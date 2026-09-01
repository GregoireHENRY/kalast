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

**Bounces.** Radiosity defaults to first order: a facet receives what its
neighbours emit and reflect directly, and light bounced twice is dropped. Pass
`bounces > 1` to `absorbed` for the Neumann series instead, one sparse matvec
per extra bounce. The error of stopping at one is of order `albedo * rowsum`
for the scattered term and `(1 - emissivity) * rowsum` for the thermal one. Didymos's self view-factor
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

    def collect(self, sim, n_facets_per_body=None):
        """Compress the chunk just rendered. Returns True when finished.

        Column counts are taken from the offsets the hemicube returns, not
        from the caller: with a view-factor proxy the columns are the *proxy's*
        facets, which is not what the caller knows about its bodies.
        `n_facets_per_body` is ignored, kept only so old calls still work.
        """
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
        dense = numpy.asarray(vf)[: hi - lo]
        edges = numpy.append(self._offsets, dense.shape[1])
        self._n_facets = numpy.diff(edges).astype(numpy.int64)

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


def delta_t_estimate(flux, temperature, emissivity):
    """Linearised surface-temperature response to an extra flux, in K.

    A radiative surface sits at `eps sigma T^4 = F`, so a small extra `dF`
    moves it by `dF / (4 eps sigma T^3)`. That is what makes an equal flux
    worth far more on the night side: at 200 K the sensitivity is four times
    what it is at 320 K.

    This is an estimate for deciding whether a term is worth carrying, not a
    substitute for stepping the model -- it ignores conduction into the
    column, which damps the response, so it is an **upper bound** on the
    steady-state move. Measured against the real runs it came out high by
    roughly a factor of two, which is the right direction for a screening
    test.
    """
    t = numpy.asarray(temperature, dtype=numpy.float64)
    denom = 4.0 * emissivity * STEFAN_BOLTZMANN * numpy.maximum(t, 1.0) ** 3
    return numpy.asarray(flux, dtype=numpy.float64) / denom


def pathological_facets(vf, body, threshold=0.5):
    """Facets whose own body fills their hemisphere: `self VF > threshold`.

    A facet with a reversed normal looks into the body and sees it almost
    everywhere, so this is a direct measurement of the defect rather than an
    inference from shape. `Mesh.inward_facing_facets` guesses at the same
    thing from the outward radial direction and, measured against this on the
    decimated Dimorphos models, over-reports by about 20x while also missing
    cases -- 22 flagged against 1 real on the 10k, 21 against 3 on the 100k.

    A genuine concavity on these meshes tops out near 0.35, so 0.5 separates
    them cleanly. What it finds is **not** repairable by flipping: reversing
    such a facet sends its self view factor to 1.0 rather than down.
    """
    return numpy.where(vf.row_sums(body) > threshold)[0]


def emitted(temperature, emissivity):
    """Thermal exitance `eps sigma T^4` per facet, W/m2."""
    t = numpy.asarray(temperature, dtype=numpy.float64)
    return emissivity * STEFAN_BOLTZMANN * t * t * t * t


def radiosity(matrix, source, reflectivity, bounces):
    """Solve `J = source + reflectivity * M J` by Neumann series.

    `matrix` must be **square** -- one body's rows against its own columns.
    That is the self block, and it is where multiple scattering lives: light
    bounces inside a concavity, and the cross-body term is both far weaker and
    not square, since a row set covers only one body.

    `bounces=1` returns `source` unchanged, which is the single-bounce model.
    Each further bounce is one more sparse matvec.

    The series converges geometrically with ratio `reflectivity * rowsum`, so
    the count needed is set by the deepest concavity rather than by the mesh
    size. On Didymos the row sums are 0.0013 and one bounce is already exact
    to 0.1 %; on Dimorphos the worst genuine concavity reaches 0.35, where the
    ratio is ~0.31 and five bounces leave 0.3 %.
    """
    source = numpy.asarray(source, dtype=numpy.float64)
    j = source
    for _ in range(max(int(bounces), 1) - 1):
        j = source + reflectivity * matrix.dot(j)
    return j


def absorbed(vf, emitted_all, reflected_all, emissivity, albedo, bounces=1,
             body=None):
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

    def scattered(values, reflectivity):
        """Inflate the body's own columns by the bounces it traps itself."""
        if bounces <= 1:
            return values
        if body is None:
            raise ValueError("bounces > 1 needs `body` -- multiple scattering "
                             "is solved on that body's square self block")
        lo = int(vf.offsets[body])
        hi = lo + int(vf.n_facets[body])
        if vf.matrix.shape[0] != hi - lo:
            raise ValueError("bounces > 1 needs one row per facet of `body`")
        out = numpy.array(values, dtype=numpy.float64, copy=True)
        out[lo:hi] = radiosity(vf.block(body), values[lo:hi],
                               reflectivity, bounces)
        return out

    if emitted_all is not None:
        # A grey surface reflects 1 - eps of the thermal IR landing on it, so
        # that is the reflectivity carrying the series.
        q += emissivity * vf.dot(scattered(emitted_all, 1.0 - emissivity))
    if reflected_all is not None:
        q += (1.0 - albedo) * vf.dot(scattered(reflected_all, albedo))
    return q


class FacetMap:
    """Maps a body's real facets onto the proxy facets used as view-factor columns.

    When the hemicube renders a decimated proxy, the columns index *proxy*
    facets, so the quantity multiplied through them -- emitted flux, reflected
    sunlight -- has to be expressed at that resolution too. Each proxy facet
    stands for the patch of real facets nearest it, and takes their
    area-weighted mean.

    The pairing is nearest-centroid. That is exact enough because a proxy is
    only ever used for the *far* field: the companion subtends tens of degrees,
    so what matters is the temperature structure across its disk -- day side
    against night side, a hundred-metre scale -- not which metre-scale facet a
    given ray struck. It would not be acceptable for the self block, where the
    view factor is dominated by immediate neighbours in a concavity and the
    proxy is exactly what smooths those away.

    Error is set by how much temperature varies within one proxy facet, so it
    is worst across a terminator.
    """

    def __init__(self, positions, proxy_positions):
        from scipy.spatial import cKDTree

        self.n_proxy = len(proxy_positions)
        _, self.index = cKDTree(numpy.asarray(proxy_positions)).query(
            numpy.asarray(positions), k=1
        )
        self.index = numpy.asarray(self.index, dtype=numpy.int64)

    def aggregate(self, values, areas):
        """Area-weighted mean of `values` over the facets behind each proxy."""
        v = numpy.asarray(values, dtype=numpy.float64)
        a = numpy.asarray(areas, dtype=numpy.float64)
        num = numpy.bincount(self.index, weights=v * a, minlength=self.n_proxy)
        den = numpy.bincount(self.index, weights=a, minlength=self.n_proxy)
        return numpy.divide(num, den, out=numpy.zeros_like(num), where=den > 0)

    def counts(self):
        """Real facets behind each proxy facet, for checking the pairing."""
        return numpy.bincount(self.index, minlength=self.n_proxy)


def synodic_period(spin_periods):
    """Period over which a pair's *relative* geometry repeats, seconds.

    Not the orbital period, which is the intuitive answer and the wrong one.
    Didymos turns 5.0299 times per Dimorphos orbit -- not an integer -- so the
    pair never repeats on the orbit alone. What repeats is
    `psi = orbital phase - spin phase`, and for a tidally locked secondary,
    whose spin period *is* its orbital period, that reduces to the beat
    between the two spin periods:

        1 / P_syn = |1 / P_a - 1 / P_b|

    For Didymos-Dimorphos, 2.821 h against an 11.368 h orbit -- four times
    shorter.

    Holds when the spin axes are aligned with the orbit normal and the orbit
    is circular. Measured against the kernels the separation is constant, so
    the orbit is circular, and the configuration recurs to 0.5-3.5 deg in
    direction and 0.7-5.7 deg in orientation -- inside the 12 deg rebuild
    cadence already accepted. The residual does not shrink with a refitted
    period, so it is a real wobble, Dimorphos's post-DART libration being the
    obvious candidate.
    """
    a, b = float(spin_periods[0]), float(spin_periods[1])
    d = abs(1.0 / a - 1.0 / b)
    if d <= 0:
        raise ValueError("the two periods are identical: no synodic beat")
    return 1.0 / d


class SynodicTable:
    """Mutual view factors precomputed over one synodic period.

    The mutual block has to be rebuilt as the pair turns -- the solid angle
    barely moves, but *which* of the companion's facets fill it, day side or
    night side, turns over completely. Rebuilding costs ~5 s and dominates a
    segment: at a 12 deg cadence a 1,309-step run spends 262 rebuilds on it.

    Since the geometry repeats synodically, those rebuilds are the same
    handful of configurations computed over and over. This holds one set per
    phase and looks them up, so a run of any length pays the build once.

    **Self blocks are stored once**, being fixed in the body frame. That
    matters less than it sounds -- measured, the mutual block carries 4.98M
    nonzeros against the self block's 251k on Dimorphos, since each of its
    facets sees ~500 Didymos facets -- but it is free to do.

    Lookup is nearest-phase, not interpolated: sparse matrices are expensive
    to blend, and at 30 entries the spacing is 12 deg, the same tolerance the
    direct rebuild cadence was measured at.

    **Measured, and not accurate enough for Didymos-Dimorphos.** Against
    direct rebuilds it costs the secondary 0.66 K in the mean, 5.7 K at p99
    and 19 K at worst, on a heating effect of 2.92 K. Doubling to 60 phases
    changes nothing (0.688 K), so the limit is not sampling: the pair does not
    recur synodically as cleanly as its geometry suggests, and Dimorphos's
    post-DART libration leaves a ~5.7 deg wobble that no density removes.
    Verified that the indexing is right -- the entry chosen matches the run's
    geometry to 5.7 deg in direction and 7.2 deg in orientation, as designed.

    The mistake in reasoning is worth keeping: 5.7 deg looked tolerable
    because the rebuild *cadence* tolerates 12 deg. But cadence error is
    staleness that returns to zero at every rebuild and averages out, while
    this is a persistent offset that never does.

    Kept for a pair that does lock rigidly, where the assumption -- spin axis
    along the orbit normal, circular orbit, no libration -- actually holds.
    """

    def __init__(self, bodies, period, epoch0, n_phases=30):
        self.bodies = list(bodies)
        self.period = float(period)
        self.epoch0 = float(epoch0)
        self.n_phases = int(n_phases)
        self.self_blocks = {}
        self.mutual = {n: [None] * self.n_phases for n in self.bodies}
        self._offsets = {}
        self._n_facets = {}

    def epoch_for(self, index):
        """The epoch whose geometry entry `index` represents."""
        return self.epoch0 + self.period * index / self.n_phases

    def index_for(self, et):
        """Nearest table entry for `et`."""
        phase = ((et - self.epoch0) / self.period) % 1.0
        return int(round(phase * self.n_phases)) % self.n_phases

    def store(self, name, index, rows):
        """Keep entry `index` for `name` from a freshly built `ViewFactors`."""
        me = self.bodies.index(name)
        if name not in self.self_blocks:
            self.self_blocks[name] = rows.block(me)
            self._offsets[name] = rows.offsets
            self._n_facets[name] = rows.n_facets
        self.mutual[name][index] = rows.block(1 - me)

    @property
    def complete(self):
        return all(m is not None for rows in self.mutual.values() for m in rows)

    def nnz(self):
        n = sum(b.nnz for b in self.self_blocks.values())
        n += sum(m.nnz for rows in self.mutual.values() for m in rows if m is not None)
        return n

    def rows(self, name, et):
        """A `ViewFactors` for `name` at `et`, from the nearest entry.

        Reassembled with the self block, so it is interchangeable with what a
        direct rebuild returns and the caller does not branch.
        """
        me = self.bodies.index(name)
        mutual = self.mutual[name][self.index_for(et)]
        blocks = [None, None]
        blocks[me] = self.self_blocks[name]
        blocks[1 - me] = mutual
        return ViewFactors(
            sparse.hstack(blocks, format="csr"),
            self._offsets[name],
            self._n_facets[name],
        )
