# Kalast CLAUDE rules

1. Read README.md.
2. README.md, CHANGELOG.md and bundle release texts are intended for users and
   should remain explicit and concise, not including devs details.
3. Kalast main goal is the thermophysical model (TPM) for binary asteroids.
4. Priority of the TPM is performance, must run as fast as possible, and
   scientific accuracy, simulating validated and trusted astrophysics and
   thermophysics.
5. Second goal of kalast is the image spacecraft simulator. A multi-purpose
   renderer has been developed further on to extend the capabilities and make
   good use of it.
6. Kalast examples should be scripts as short as possible and human maintainable
   especially for Python users. Most of kalast usage is started from a script.
7. The UI app extends flexibility and interactions with the setup and outcome of
   the executed script.
8. The UI app should not slow down the script execution in any way.
9. Kalast core is written in Rust. This is really important to ensure trust in
   TPM implementation and best possible performance.
10. Any kind of performance improvement should be reached to make the core
    simulation run as fast as possible without compromising the physics.
11. Kalast is made user friendly for Python users with a Pythonic wrapper.
12. The python wrapper must never influence the core implementation in Rust to
    be slower in any way.
13. Kalast should always remain usable 100% in Rust only without Python. Python
    is optional.
14. No feature should be implemented in Python directly, Python is just wrapping
    Rust features.
15. Always write notes about features implementations concerning TPM/renderer.
16. Maintain timeline updated with recent features in notes.
17. Maintain a list of unfinished tasks and where work stopped.
18. Maintain API.md, CONFIG.md, CONTROLS.md concise and up to date.
19. After a pull, read the pulled notes, especially potential handoff work.
20. Never delete without asking a file that was created before by the user.
21. Use scratch space or temporary folder. Put throwaway scripts under `/tmp`,
    not in the repo. Copy the example, instrument the copy, run it from there.
    That keeps `examples/` clean, and it means the scratch directory is one this
    session created and may therefore delete without asking (see above) — which
    the project's own directories are not. Do not benchmark by editing an
    example in place: the shortened sweep and the rate prints are not changes
    anyone wants committed.
22. Paths in examples are mostly local to users, let users change them, do not
    try to fix without asking.
23. On Windows, build with `python tools/develop.py` rather than calling
    maturin directly. A mapped DLL cannot be written on Windows, and VS Code's
    language server keeps `kalast/_rs.pyd` mapped for the whole editor session,
    so plain `maturin develop` throws the finished build away at the copy step
    with `os error 32`. The wrapper renames the old module aside first --
    renaming a mapped image is allowed -- and passes everything through, so
    `python tools/develop.py --release` works too. macOS does not need it and is
    unaffected either way.
24. Never run against the project's real output directories. Frame export
    defaults to `out/frames`, so a benchmark left at the default writes into
    whatever a real run is using, and two exporters pointed at one directory
    race on the startup index scan as well as on cleanup. Always redirect:
    ```python
    app.config.export_dir = "/tmp/<something>/frames"
    ```
25. `app.config.vsync` must stay `False`. Otherwise the loop reports the
    display refresh rate rather than anything about the code — this produced a
    "3.1M facets costs 2x" conclusion that was entirely an artifact of a 120 Hz
    panel. It is the **default** since 15 September, so this is now a matter of
    not switching it on rather than remembering to switch it off; scripts that
    still set it explicitly are harmless.
26. Take medians over repeats and discard the first run after a rebuild.
27. Run tests as a background window that do not take mouse input focus over
    current work of the user.
28. Automatically generate and maintain up to date python stubs for the wrapper.
    ```sh
    python tools/gen_stubs.py            # after changing any #[pyclass]
    python tests/test_stubs.py           # checks they are current
    ```
29. The editor's config panel: `src/app/gui/config_panel.rs` is generated from
    `src/app/config.rs`, not written by hand.
    ```sh
    python tools/gen_config_panel.py     # after adding a field to Config
    python tests/test_config_panel.py    # checks it is current and complete
    ```
    Two guards, both needed: the committed file must match what the generator
    produces, and every field must appear in it. The second catches the case
    that matters -- a new option with no widget is invisible, because the panel
    still looks complete.
30. The struct's nesting is the grouping. `Config` is a struct of sub-structs --
    `shading`, `light`, `shadows`, `grid`, ... -- and the generator emits one
    function per group. There is no prefix table and no `:group:` marker: to
    move a field between headers, move it between structs, and the panel, the
    Python surface and the docs all follow. `src/app/gui/simulation_panel.rs`
    composes those functions under topic headers by hand, each beside the entity
    it describes -- the Sun's position and the Sun's colour under one header.
    Which groups share a header is decided there and nowhere else. The Rust doc
    comments carry what the type cannot: | marker | effect | |---|---| | `///
    :range: 0..=16` | a slider with those bounds instead of a drag field | |
    `/// :step: 0.01` | drag speed | | `/// :skip:` | no widget; for things
    edited from a script, like `colormap` | | `/// :py_custom:` | no generated
    Python accessor; see the next section | Otherwise the widget follows the
    type, and the first sentence of the doc becomes the hover text -- so
    documenting a field in Rust documents it in the UI.
31. The Python getters, generated too -- and the trap that made them so.
    The `#[getter]`/`#[setter]` pairs were the one mirror of the config still
    written by hand, and it showed: a field could be complete everywhere --
    widget in the editor, line in the `.pyi` -- and still raise `AttributeError`
    from a script, which the stubs cannot catch because they are generated
    from the wrapper. That bit four times (`debug_light_cube_fit`,
    `facet_labels`, `selection_color`, `colorbar_border`). Since 15 September:
    ```sh
    python tools/gen_bindings.py            # after adding a field to Config
    python tests/test_config_bindings.py    # every field reachable and writable
    ```
    `src/py/app/config_gen.rs` holds one view class per group, each carrying the
    same `Rc<RefCell<Config>>` as the root and reading its own group through it.
    **That indirection is why these are generated rather than being
    `#[pyclass(get_all)]` on the sub-structs**: a `get_all` getter hands Python
    a *copy*, so `config.grid.color = ...` on a copy sets nothing, silently.
    Every accessor has to go through the shared handle, which is a page of
    identical code per group -- exactly the thing to generate.
    A field with real logic opts out with `/// :py_custom:` and is written by
    hand in `src/py/app/config.rs` -- today only `data.colormap`, which parses
    names and arrays. `tests/test_config_bindings.py` still checks those, since
    a forgotten hand-written accessor is the old bug back.
    Old flat names -- `config.grid_color`, `config.vsync` -- keep working for
    one release through a `__getattr__`/`__setattr__` shim on the root,
    generated from `tools/config_renames.py`, with a `DeprecationWarning` naming
    the new path. Three that moved to `app.config` (`title`, `fullscreen`,
    `vsync`) raise an error saying so instead.
32. `notes/` holds dated write-ups (`YYYY-MM-DD_topic`). Some are undated on
    purpose, because they are living documents rather than a record of one day:
    - `TIMELINE.md` — the running summary, including what is open and what was
      deliberately paused.
    - `CONFIG.md` — the `app.config` reference. Add an entry here whenever a
      config option is added, or it goes stale silently.
    - `CONTROLS.md` — keyboard and mouse bindings for the render window. Same
      rule: add to it whenever a binding is added.
    - `API.md` — the Python API outside the config: `App`, `sim.state`,
      `sim.huds`, bodies, camera and Sun, and the GPU-result queries. Same rule
      again: add to it whenever something is exposed to Python.
33. Releasing. The GitHub release text is `CHANGELOG.md`'s section for that
    version, verbatim, and nothing else: a list of what changed for the people
    who use kalast, no prose. Release engineering, CI, caching, and refactors
    that change nothing a user sees do not go in it -- those belong in `notes/`.
    The version gate refuses a tag whose section is missing or has no `- `
    entry. The order is: bump the three manifests, write the section, push,
    rehearse (`gh workflow run release.yml`), and if green, show the section
    to the user and get it approved before pushing the tag -- every tag, not
    just the first. A green rehearsal is the tag's technical go; the changelog
    review is its editorial one, and it is the user's, not yours. A tag whose
    commit was rehearsed publishes the rehearsal's artefacts, so the tag run is
    minutes.