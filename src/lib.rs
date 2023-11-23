/*!
# Thermophysical Model for Binary Asteroids

Latest executables of **kalast** [can be downloaded here][releases].

*kalast* can also be used as a library in Rust.
Add the dependency to your `Cargo.toml`:

```toml
[dependencies]
kalast = "0.3"
```

and have a look at [the examples][examples] or [the different modules here][crate#modules].

Information on the configuration of your **kalast** scenarios are located at the documentation of
[the module `cfg`][mod@cfg].

[examples]: https://github.com/GregoireHENRY/kalast/tree/main/examples
[releases]: https://github.com/GregoireHENRY/kalast/releases
*/

pub mod ast;
pub mod cfg;
pub mod prelude;
pub mod simu;
pub mod thermal;
pub mod util;
pub mod win;
