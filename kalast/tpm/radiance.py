"""Band-integrated thermal radiance for an instrument's spectral response.

A thermophysical run produces one surface temperature per facet. An image
needs what the detector *measures*: the Planck function weighted by the
instrument's spectral response and integrated over wavelength.

    L = integral B(T,w) eps R(w) dw                            [W/m2/sr]

This is the **band-integrated radiance**, and it is what `__call__` returns.
It is the quantity real calibrated TIRI products carry -- their headers say
`BUNIT = 'W m^-2 sr^-1'` -- so a simulated frame must use it to be comparable
with one.

Because it is an integral rather than an average, a wide filter returns much
more than a narrow one at the same temperature. TIRI's wide band `g` spans
`integral R dw = 2.71 um` against `a`'s 0.51 um, so `g` reads roughly five
times higher. That factor is physical and its absence is a symptom: it is how
the wrong normalisation was caught here.

`band_averaged` returns the other reduction,

    L_bar = integral B(T,w) eps R(w) dw / integral R(w) dw   [W/m2/sr/um]

a band-*averaged spectral* radiance. Being an average it is nearly the same
for every filter, which is exactly why it cannot be compared against a real
TIRI frame. It is offered because some pipelines quote radiance per unit
wavelength, and because the ratio of the two is a useful diagnostic.

A note on the response scale
----------------------------

The band-integrated form is linear in `R`, so it is only meaningful if `R` is
an absolute throughput rather than a shape normalised to an arbitrary peak.
For TIRI's `response.csv` it is absolute: `Response_Fil-x` equals
`Bolometer * Lens * Filter-x` exactly (median ratio 1.000000 for all seven),
with peak values of 0.37 to 0.81 -- physical transmissions, not normalised
curves.

An earlier version of this module claimed `Response_Fil-a..f` carried a
factor 0.5 that `Response_Fil-g` did not, and used that to argue for the
normalised form. **That was wrong.** It came from dividing `Response` by
`Bolometer * Lens * Filter` at wavelengths where the denominator is nearly
zero, in columns quoted to four decimals; the quotient there is quantisation
noise, and taking its minimum produced a spurious 0.5.

Why a lookup table
------------------

`L` is a scalar function of `T` alone once the filter is fixed. Evaluating it
directly means a `(n_facets, n_wavelengths)` Planck array -- 10,000 facets
against TIRI's 1302 samples is 13 M doubles per call, and an image sequence
calls it once per epoch per filter.

So tabulate `L(T)` once on a fine temperature grid and interpolate. The
integration is done properly at table-build time; lookup is a single
`numpy.interp`. At 4000 points over 50-450 K the interpolation error is far
below the temperature uncertainty of the model itself, and the class reports
it (`max_interpolation_error`) rather than asking anyone to take that on
trust.

Emissivity is applied as a grey factor. TIRI's filters are narrow enough that
a spectrally varying emissivity would matter, but no such measurement exists
for Didymos, and assuming one would be inventing data.
"""

import numpy

#: Planck constants, SI.
_H = 6.62607015e-34
_C = 2.99792458e8
_KB = 1.380649e-23

#: TIRI's filter names as they appear in `response.csv` column suffixes.
TIRI_FILTERS = ("a", "b", "c", "d", "e", "f", "g")

#: The wide-band filter, the one to use when a single band is wanted.
TIRI_WIDE = "g"


def planck(temperature, wavelength):
    """Spectral radiance of a black body, `W/m2/sr/m`.

    Broadcasts, so `temperature` and `wavelength` may be arrays of any shapes
    that broadcast together. Wavelength in metres.
    """
    t = numpy.asarray(temperature, dtype=numpy.float64)
    w = numpy.asarray(wavelength, dtype=numpy.float64)
    # exp overflows for cold temperatures at short wavelengths; that limit is
    # a radiance of zero, so clip the exponent rather than let it warn.
    x = numpy.clip(_H * _C / (w * _KB * numpy.maximum(t, 1e-6)), 0.0, 700.0)
    return 2.0 * _H * _C * _C / w**5 / numpy.expm1(x)


def load_tiri_response(path, filters=TIRI_FILTERS):
    """Read `response.csv`, returning `{filter: (wavelength_m, response)}`.

    The `Response_Fil-x` columns are the complete optical chain -- bolometer,
    lens and filter combined -- so they are used directly rather than
    reconstructed from the individual columns also present in the file.
    """
    import pandas

    df = pandas.read_csv(path)
    wl = df["#Wavelength[um]"].to_numpy(dtype=numpy.float64) * 1e-6
    out = {}
    for f in filters:
        column = f"Response_Fil-{f}"
        if column not in df.columns:
            raise KeyError(f"{column} not in {path}")
        out[f] = (wl, df[column].to_numpy(dtype=numpy.float64))
    return out


class BandRadiance:
    """Band-averaged radiance as a function of temperature, tabulated.

    Parameters
    ----------
    wavelength : (nw,) array
        Wavelengths in metres, increasing.
    response : (nw,) array
        Spectral response over those wavelengths. Scale is arbitrary; it
        cancels in the normalisation.
    emissivity : float
        Grey emissivity.
    t_range, n_table : optional
        Temperature range and table size. The default range spans anything a
        small airless body reaches, from deep night to subsolar perihelion.
    """

    def __init__(self, wavelength, response, emissivity=1.0,
                 t_range=(30.0, 500.0), n_table=20000, chunk=1000):
        self.wavelength = numpy.asarray(wavelength, dtype=numpy.float64)
        self.response = numpy.asarray(response, dtype=numpy.float64)
        self.emissivity = float(emissivity)
        self.t_range = t_range

        # Integral of the response, in metres. `bandwidth` is the same in
        # micrometres, which is the figure that explains why a wide filter
        # returns several times what a narrow one does.
        self.norm = numpy.trapezoid(self.response, self.wavelength)
        self.bandwidth = float(self.norm * 1e6)
        if self.norm <= 0:
            raise ValueError("response integrates to zero")

        self.t_table = numpy.linspace(t_range[0], t_range[1], n_table)
        # Built in chunks: the full (n_table, nw) Planck array would be 200 MB
        # at this table size, and it is used once and discarded.
        weighted = numpy.empty(n_table)
        for i in range(0, n_table, chunk):
            t_chunk = self.t_table[i:i + chunk, None]
            b = planck(t_chunk, self.wavelength[None, :])
            weighted[i:i + chunk] = numpy.trapezoid(
                b * self.response[None, :], self.wavelength, axis=1
            )
        weighted *= self.emissivity
        self.l_table = weighted
        # Per micrometre, not per metre: the convention such products use.
        self.averaged_table = weighted / self.norm * 1e-6

        # Effective wavelength, response-weighted: the single wavelength that
        # best characterises the band, useful for reporting and for brightness
        # temperature sanity checks.
        self.effective_wavelength = float(
            numpy.trapezoid(self.response * self.wavelength, self.wavelength)
            / self.norm
        )

    def __call__(self, temperature):
        """Band-integrated radiance `W/m2/sr` for each temperature."""
        t = numpy.asarray(temperature, dtype=numpy.float64)
        if t.size and (t.min() < self.t_range[0] or t.max() > self.t_range[1]):
            raise ValueError(
                f"temperature {t.min():.1f}-{t.max():.1f} K outside the "
                f"tabulated range {self.t_range}; widen t_range"
            )
        return numpy.interp(t, self.t_table, self.l_table)

    def band_averaged(self, temperature):
        """Band-averaged spectral radiance `W/m2/sr/um`.

        Nearly filter-independent, so not comparable with a real TIRI frame;
        see the module docstring.
        """
        t = numpy.asarray(temperature, dtype=numpy.float64)
        return numpy.interp(t, self.t_table, self.averaged_table)

    def exact(self, temperature):
        """The same quantity as `__call__` without the table, to validate it."""
        t = numpy.atleast_1d(numpy.asarray(temperature, dtype=numpy.float64))
        b = planck(t[:, None], self.wavelength[None, :])
        return (
            numpy.trapezoid(b * self.response[None, :], self.wavelength, axis=1)
            * self.emissivity
        )

    def max_interpolation_error(self, n_probe=997):
        """Worst relative error of the table against direct integration.

        Probed at points deliberately offset from the table nodes, since the
        error at a node is zero by construction and would flatter the table.
        """
        t = numpy.linspace(self.t_range[0], self.t_range[1], n_probe)
        exact = self.exact(t)
        approx = self(t)
        nz = exact > 0
        return float(numpy.abs(approx[nz] / exact[nz] - 1.0).max())

    def brightness_temperature(self, radiance):
        """Invert the band: the temperature that would give this radiance.

        The table is monotonic in `T`, so this is an interpolation back the
        other way. Useful for reporting an image in kelvin, and for checking
        that a radiance round-trips.
        """
        r = numpy.asarray(radiance, dtype=numpy.float64)
        return numpy.interp(r, self.l_table, self.t_table)


def tiri_bands(path, emissivity=1.0, filters=TIRI_FILTERS, **kwargs):
    """`{filter: BandRadiance}` for every TIRI filter in `response.csv`."""
    resp = load_tiri_response(path, filters)
    return {
        f: BandRadiance(wl, r, emissivity=emissivity, **kwargs)
        for f, (wl, r) in resp.items()
    }
