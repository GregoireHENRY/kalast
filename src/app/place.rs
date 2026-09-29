//! Where the window opens: on the screen `app.config.monitor` names, else
//! the one the UI app was last closed on (`settings::Place`), else the main
//! one -- at the size `app.config.width` and `height` give, else the size it
//! had there, else most of the screen.
//!
//! On plain numbers rather than winit's monitors, so that it can be tested
//! without a screen.

use crate::app::settings::Place;

/// A screen, as the placement needs it: winit's `MonitorHandle`, reduced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screen {
    pub name: String,
    /// Its origin on the desktop and its size, in physical pixels.
    pub origin: (i32, i32),
    pub size: (u32, u32),
}

/// Where the window opens and how big, in physical pixels on the desktop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Opening {
    pub screen: usize,
    pub position: (i32, i32),
    pub size: (u32, u32),
    pub maximized: bool,
}

/// The screen `spec` names: its number in the list, from 1, or part of its
/// name, whatever the case.
pub fn named(screens: &[Screen], spec: &str) -> Option<usize> {
    let spec = spec.trim();
    if spec.is_empty() {
        return None;
    }
    if let Ok(n) = spec.parse::<usize>() {
        return (1..=screens.len()).contains(&n).then(|| n - 1);
    }
    let spec = spec.to_lowercase();
    screens.iter().position(|s| s.name.to_lowercase().contains(&spec))
}

/// The screen the window was left on: the same name at the same place on
/// the desktop -- two screens of one model share a name -- else the same
/// name, the screens arranged anew, else whatever is at that place now.
fn left_on(screens: &[Screen], place: &Place) -> Option<usize> {
    let same_name = |s: &Screen| !place.monitor.is_empty() && s.name == place.monitor;
    screens
        .iter()
        .position(|s| same_name(s) && s.origin == place.origin)
        .or_else(|| screens.iter().position(same_name))
        .or_else(|| screens.iter().position(|s| s.origin == place.origin))
}

/// Where the window opens. `monitor`, `at` and `size` are the config's --
/// empty, negative and `0` when not set -- and win over `left`, where the
/// UI app was last closed, which applies on its own screen only. With
/// neither, the window is `fraction` of the main screen, in its middle.
/// `None` without a screen to ask: headless, or a compositor that says
/// nothing.
pub fn opening(
    screens: &[Screen],
    primary: Option<usize>,
    monitor: &str,
    at: (i32, i32),
    size: (u32, u32),
    left: Option<&Place>,
    fraction: f32,
) -> Option<Opening> {
    if screens.is_empty() {
        return None;
    }
    let was = left.and_then(|p| left_on(screens, p));
    let screen = named(screens, monitor).or(was).or(primary).unwrap_or(0);
    let here = left.filter(|_| was == Some(screen));
    let s = &screens[screen];
    // A size left behind is kept inside the screen, which may have become
    // smaller since; one the config gives is taken as it is.
    let kept = here.and_then(|p| p.size);
    let auto = |full: u32, least: u32| ((full as f32 * fraction) as u32).max(least);
    let w = match (size.0, kept) {
        (0, Some((w, _))) => w.min(s.size.0),
        (0, None) => auto(s.size.0, 320),
        (w, _) => w,
    };
    let h = match (size.1, kept) {
        (0, Some((_, h))) => h.min(s.size.1),
        (0, None) => auto(s.size.1, 240),
        (h, _) => h,
    };
    let room = ((s.size.0 as i32 - w as i32).max(0), (s.size.1 as i32 - h as i32).max(0));
    // Each axis on its own: the config's, else where it was left, else the
    // middle -- on the screen, whole where it fits, its title bar in reach.
    let was = here.and_then(|p| p.position);
    let x = if at.0 >= 0 { at.0 } else { was.map_or(room.0 / 2, |p| p.0) };
    let y = if at.1 >= 0 { at.1 } else { was.map_or(room.1 / 2, |p| p.1) };
    let (x, y) = (x.clamp(0, room.0), y.clamp(0, room.1));
    Some(Opening {
        screen,
        position: (s.origin.0 + x, s.origin.1 + y),
        size: (w, h),
        maximized: here.is_some_and(|p| p.maximized),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screens() -> Vec<Screen> {
        vec![
            Screen { name: "Built-in Retina Display".into(), origin: (0, 0), size: (3024, 1964) },
            Screen { name: "DELL U2720Q".into(), origin: (3024, -400), size: (3840, 2160) },
            Screen { name: "DELL U2720Q".into(), origin: (6864, -400), size: (3840, 2160) },
        ]
    }

    fn left(monitor: &str, origin: (i32, i32)) -> Place {
        Place { monitor: monitor.into(), origin, position: Some((100, 50)), size: Some((2000, 1200)), maximized: false }
    }

    #[test]
    fn a_screen_is_named_by_number_or_by_part_of_its_name() {
        let s = screens();
        assert_eq!(named(&s, "1"), Some(0));
        assert_eq!(named(&s, " 3 "), Some(2));
        assert_eq!(named(&s, "4"), None);
        assert_eq!(named(&s, "0"), None);
        assert_eq!(named(&s, "dell"), Some(1));
        assert_eq!(named(&s, "Built-in"), Some(0));
        assert_eq!(named(&s, "LG"), None);
        assert_eq!(named(&s, ""), None);
    }

    /// Nothing remembered, nothing asked: the main screen, the window in
    /// its middle.
    #[test]
    fn at_first_the_main_screen() {
        let o = opening(&screens(), Some(0), "", (-1, -1), (0, 0), None, 0.85).unwrap();
        assert_eq!((o.screen, o.size, o.maximized), (0, (2570, 1669), false));
        assert_eq!(o.position, ((3024 - 2570) / 2, (1964 - 1669) / 2));
        assert_eq!(opening(&[], None, "", (-1, -1), (0, 0), None, 0.85), None, "no screen to ask");
    }

    /// Where it was left: the second of two alike screens, told apart by
    /// where they are, at the size and place it had there.
    #[test]
    fn where_it_was_left() {
        let place = left("DELL U2720Q", (6864, -400));
        let o = opening(&screens(), Some(0), "", (-1, -1), (0, 0), Some(&place), 0.85).unwrap();
        assert_eq!((o.screen, o.position, o.size), (2, (6964, -350), (2000, 1200)));

        // Its screen moved on the desktop: found by name, the place on it kept.
        let moved = left("Built-in Retina Display", (500, 500));
        let o = opening(&screens(), Some(1), "", (-1, -1), (0, 0), Some(&moved), 0.85).unwrap();
        assert_eq!((o.screen, o.position), (0, (100, 50)));

        // Its screen is gone: the main one, in the middle, at its own size.
        let gone = left("LG UltraFine", (-2560, 0));
        let o = opening(&screens(), Some(0), "", (-1, -1), (0, 0), Some(&gone), 0.85).unwrap();
        assert_eq!((o.screen, o.size), (0, (2570, 1669)));

        let maximized = Place { maximized: true, ..place.clone() };
        assert!(opening(&screens(), Some(0), "", (-1, -1), (0, 0), Some(&maximized), 0.85).unwrap().maximized);
    }

    /// A screen that has become smaller keeps the window on it, whole.
    #[test]
    fn a_place_off_the_screen_comes_back_onto_it() {
        let far = Place { position: Some((2900, 1900)), size: Some((5000, 1000)), ..left("Built-in Retina Display", (0, 0)) };
        let o = opening(&screens(), Some(0), "", (-1, -1), (0, 0), Some(&far), 0.85).unwrap();
        assert_eq!((o.size, o.position), ((3024, 1000), (0, 964)));
        let above = Place { position: Some((-50, -30)), ..left("Built-in Retina Display", (0, 0)) };
        assert_eq!(opening(&screens(), Some(0), "", (-1, -1), (0, 0), Some(&above), 0.85).unwrap().position, (0, 0));
    }

    /// The config wins: the screen it names, the size it gives -- and what
    /// was left applies only when it names the same screen.
    #[test]
    fn the_config_wins() {
        let place = left("Built-in Retina Display", (0, 0));
        let o = opening(&screens(), Some(0), "dell", (-1, -1), (0, 0), Some(&place), 0.85).unwrap();
        assert_eq!((o.screen, o.size), (1, (3264, 1836)), "another screen: its own size, in its middle");
        assert_eq!(o.position, (3024 + (3840 - 3264) / 2, -400 + (2160 - 1836) / 2));

        let o = opening(&screens(), Some(1), "1", (-1, -1), (0, 0), Some(&place), 0.85).unwrap();
        assert_eq!((o.screen, o.position, o.size), (0, (100, 50), (2000, 1200)), "the same screen: where it was");

        let o = opening(&screens(), Some(0), "", (-1, -1), (1600, 900), Some(&place), 0.85).unwrap();
        assert_eq!((o.screen, o.position, o.size), (0, (100, 50), (1600, 900)), "the size given, where it was");

        let o = opening(&screens(), Some(0), "LG", (-1, -1), (0, 0), Some(&place), 0.85).unwrap();
        assert_eq!(o.screen, 0, "a screen that is not there: where it was left");

        let o = opening(&screens(), Some(0), "2", (40, -1), (1600, 900), None, 0.85).unwrap();
        assert_eq!((o.screen, o.position, o.size), (1, (3024 + 40, -400 + (2160 - 900) / 2), (1600, 900)), "x given, y in the middle");
        let o = opening(&screens(), Some(0), "", (9000, 30), (0, 0), Some(&place), 0.85).unwrap();
        assert_eq!(o.position, (3024 - 2000, 30), "kept on the screen");
    }
}
