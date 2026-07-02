use kalast;
// use kalast::Vec3;

fn main() {
    let config = kalast::app::config::Config {
        debug_app: true,

        ..Default::default()
    };

    let mut app = kalast::app::App::new_with_config(config);

    let handle = app.handle();

    std::thread::spawn(move || {
        handle.pause();

        loop {
            if handle.snapshot().state.is_paused {
                break;
            }
            // std::thread::sleep(std::time::Duration::from_millis(10));

            // println!("[MAIN] paused at #{}", handle.iteration());
        }

        handle.modify(|s| {
            println!("yo: {}", s.state.is_paused);
        });

        handle.resume();

        for _ in 0..5 {
            let s = handle.snapshot();
            println!(
                "[MAIN] iteration={}, bodies={}",
                s.state.iteration,
                s.bodies_state.len()
            );
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    });

    app.start();
}
