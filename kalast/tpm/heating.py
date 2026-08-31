"""Self- and mutual heating: turning view factors into an absorbed flux.

`sim.request_hemicube` measures what fraction of each facet's sky is filled by
every other facet in the scene -- see `notes/2026-08-27_conduction_solvers/`
section 9.3. This module is what consumes that: it stores the result in a form
a long run can afford, and adds the two terms it implies to the surface
balance.

**Why sparse.** The matrix is nominally `(n_facets, n_total)` -- 10,000 by
20,000 for the Didymos pair, 0.80 GB dense in float32, and a dense matvec per
timestep on top. Measured on Didymos at the study epoch it is 0.31 % dense:
mean 63 nonzeros per row, median 11. Stored as CSR that is about 5 MB and the
matvec is ~1 ms. So the dense form is never assembled in full; rows are
collected in chunks and compressed as they arrive.

**Why single-bounce.** Radiosity is solved to first order only: a facet
receives what its neighbours emit and reflect directly, and light bounced
twice is dropped. The error is of order `albedo * rowsum` for the scattered
term and `emissivity * rowsum` for the thermal one. Didymos's self view-factor
row sums are mean 0.0013 and max 0.043, so the second bounce is below 0.4 %
everywhere on it. That is not uniformly true -- Dimorphos reaches 0.35 in a
genuine concavity, where a second bounce would be a ~30 % correction to a term
that is itself small. Recorded rather than hidden: the deep concavities are
the places this approximation is weakest.
"""

import numpy
from scipy import sparse

from kalast.util import STEFAN_BOLTZMANN

# Values below this are dropped when compressing. Measured on Didymos: at
# 1e-7 the row sums are preserved to every digit and the nonzero count does
# not move, while 1e-6 already costs 1.5 % of the mean row sum. The GPU
# accumulator is fixed point, so exact zeros are exact.
THRESHOLD = 1.0e-7


class ViewFactors:
    """Sparse view-factor rows for one body, over the shared column space.

    Rows are that body's facets in the order they were requested. Columns span
    *every* loaded body, `offsets[b]` giving where body `b` starts, so one
    object carries the self and mutual terms together and splitting them is a
    column slice. Row sums are at most 1; the shortfall radiates to space.
    """

    def __init__(self, matrix, offsets, n_facets):
        self.matrix = matrix.tocsr()
        self.offsets = numpy.asarray(offsets, dtype=numpy.int64)
        self.n_facets = numpy.asarray(n_facets, dtype=numpy.int64)

    @property
    def n_bodies(self):
        return len(self.offsets)

    @property
    def nnz(self):
        return self.matrix.nnz

    def block(self, body):
        """The columns belonging to `body`, as its own sparse matrix."""
        lo = int(self.offsets[body])
        return self.matrix[:, lo:lo + int(self.n_facets[body])]

    def row_sums(self, body=None):
        m = self.matrix if body is None else self.block(body)
        return numpy.asarray(m.sum(axis=1)).ravel()

    def dot(self, values):
        """Multiply by a vector laid out over the shared column space."""
        return self.matrix.dot(numpy.asarray(values, dtype=numpy.float64))

    def stack(self, per_body):
        """Concatenate one array per body into the shared column layout.

        The TPM keeps each body's state separately and on its own grid, so the
        quantity the matvec needs -- emitted flux, reflected sunlight -- has to
        be laid back out in the order the columns are in.
        """
        out = numpy.zeros(self.matrix.shape[1], dtype=numpy.float64)
        for b, v in enumerate(per_body):
            if v is None:
                continue
            lo = int(self.offsets[b])
            out[lo:lo + int(self.n_facets[b])] = v
        return out

    def save(self, path):
        m = self.matrix.tocsr()
        numpy.savez_compressed(
            str(path), data=m.data, indices=m.indices, indptr=m.indptr,
            shape=numpy.asarray(m.shape), offsets=self.offsets,
            n_facets=self.n_facets,
        )

    @classmethod
    def load(cls, path):
        z = numpy.load(str(path))
        m = sparse.csr_matrix(
            (z["data"], z["indices"], z["indptr"]), shape=tuple(z["shape"])
        )
        return cls(m, z["offsets"], z["n_facets"])


class ViewFactorBuilder:
    """Drives `request_hemicube` across frames, compressing as rows arrive.

    The request/read pair straddles a render -- requested in `before_render`,
    available in `after_render` -- so a full matrix cannot be built inside one
    call. This holds the position between frames: call `request` from
    `before_render` and `collect` from `after_render`, and check `done`.

    Rows come back dense, `(chunk, n_total)`. `chunk` therefore sets the peak
    memory: 1,000 rows over 20,000 columns is 80 MB, against 0.80 GB for all
    10,000 at once.
    """

    def __init__(self, body, n_facets, facets=None, resolution=128,
                 batch=256, chunk=1000, threshold=THRESHOLD):
        self.body = body
        self.facets = (numpy.arange(n_facets, dtype=numpy.uint32)
                       if facets is None
                       else numpy.asarray(facets, dtype=numpy.uint32))
        self.resolution = resolution
        self.batch = batch
        self.chunk = chunk
        self.threshold = threshold
        self._at = 0
        self._pending = None
        self._parts = []
        self._offsets = None
        self._n_facets = None
        self.result = None

    @property
    def done(self):
        return self.result is not None

    @property
    def progress(self):
        return self._at / max(len(self.facets), 1)

    def request(self, sim):
        if self.done or self._pending is not None:
            return
        lo = self._at
        if lo >= len(self.facets):
            return
        hi = min(lo + self.chunk, len(self.facets))
        self._pending = (lo, hi)
        sim.request_hemicube(
            body=self.body, facets=self.facets[lo:hi],
            resolution=self.resolution, batch=self.batch,
        )

    def collect(self, sim, n_facets_per_body):
        """Compress the chunk just rendered. Returns True when finished."""
        if self.done or self._pending is None:
            return self.done
        got = sim.hemicube()
        if got is None:
            # The frame did not carry the request through; retry it rather
            # than silently leaving a band of the matrix zero.
            self._pending = None
            return False
        vf, offsets = got
        lo, hi = self._pending
        self._pending = None
        self._offsets = numpy.asarray(offsets, dtype=numpy.int64)
        self._n_facets = numpy.asarray(n_facets_per_body, dtype=numpy.int64)

        dense = numpy.asarray(vf)[: hi - lo]
        dense[dense < self.threshold] = 0.0
        self._parts.append(sparse.csr_matrix(dense))
        self._at = hi

        if self._at >= len(self.facets):
            self.result = ViewFactors(
                sparse.vstack(self._parts, format="csr"),
                self._offsets, self._n_facets,
            )
            self._parts = []
        return self.done


def emitted(temperature, emissivity):
    """Thermal exitance `eps sigma T^4` per facet, W/m2."""
    t = numpy.asarray(temperature, dtype=numpy.float64)
    return emissivity * STEFAN_BOLTZMANN * t * t * t * t


def absorbed(vf, emitted_all, reflected_all, emissivity, albedo):
    """Extra absorbed flux on each row facet, W/m2.

    Two terms, both first order:

        eps_i     * sum_j VF_ij eps_j sigma T_j^4     thermal re-radiation
        (1 - A_i) * sum_j VF_ij A_j S_j               scattered sunlight

    `emitted_all` and `reflected_all` are laid out over the shared column
    space -- use `ViewFactors.stack`. `S_j` is the sunlight *incident* on
    facet `j`, before its own albedo, and must already carry the shadowing:
    a facet in eclipse reflects nothing.

    The thermal term is absorbed with the facet's emissivity rather than
    `1 - albedo`, since at these wavelengths Kirchhoff's law makes absorptivity
    and emissivity the same number, and the visible albedo is not it.
    """
    q = numpy.zeros(vf.matrix.shape[0], dtype=numpy.float64)
    if emitted_all is not None:
        q += emissivity * vf.dot(emitted_all)
    if reflected_all is not None:
        q += (1.0 - albedo) * vf.dot(reflected_all)
    return q
