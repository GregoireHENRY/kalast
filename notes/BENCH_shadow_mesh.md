# Shadow-mesh proxies — superseded

This note's measurements were single runs and its headline ("100k captures
~99% of the available gain") did not survive a more careful repeat: with 3
repeats per configuration across two scene geometries, and machine-throttling
outliers identified and excluded, the figure is **90-96%**.

**Read `shadow_mesh_comparison/README.md` instead.** It supersedes this file
entirely: same question, better controlled, with renders, figures and the
error analysis.

The release-build finding that used to live here (use `--release`; `lto` and
`codegen-units` buy nothing) has moved to the Compilation section of
`README.rst`, which is where build instructions belong.
