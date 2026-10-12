//! The UI app's scripts on a thread of their own.
//!
//! The window, its panels and the scene's drawing stay on the main thread,
//! where the platform wants them; scripts, their `before_render` and
//! `after_render`, and the python tab run on this one. A slow script -- a
//! thermophysical step in Python, a long loop at the console -- then holds
//! this thread and not the panels, which go on at the display's pace.
//!
//! The two meet in the scene, which is `Locked` (`crate::locked`), and in
//! messages: the main thread asks for a script to be run, the console served,
//! a callback called; the script thread asks for a frame -- a script's
//! `app.step()` -- and says when what it was asked is done. A frame waits for
//! its callbacks without holding the panels, a turn of the loop at a time,
//! as it waits for its bands (`Window::render`).
//!
//! The thread itself is Python's, so it is started by the bindings
//! (`py::app::script`); this is the engine's half.

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::time::Duration;

/// What the main thread asks of the script thread.
pub enum ToScript {
    /// Run a script, from the editor's Play or Restart.
    Run { path: String, source: String },
    /// Serve the python tab: the lines typed, a Tab.
    Console,
    /// Call `before_render` (or `after_render`) for the frame being drawn.
    Callback { before: bool, dt: crate::Float },
    /// The frame a script's `step()` asked for is drawn: whether the window
    /// is still open and the run not superseded.
    Frame { running: bool },
}

/// What the script thread says back.
pub enum ToMain {
    /// The script asked for returned.
    Ran,
    /// The python tab was served.
    Served,
    /// The callback asked for returned.
    Called,
    /// A script's `step()`: draw a frame.
    Step,
}

/// The main thread's end: what it has asked and not had back.
pub struct Main {
    tx: Sender<ToScript>,
    rx: Receiver<ToMain>,
    /// A script is running there: frames are drawn when it steps.
    running: bool,
    /// A callback asked for and not yet returned, and one returned whose
    /// answer `call` has not yet taken: the end of a turn may read it.
    calling: bool,
    called: bool,
    /// The python tab asked to be served and not yet.
    serving: bool,
    /// A step asked for and its frame not yet answered.
    step_asked: bool,
}

/// The script thread's end, which the bindings' `App` steps through.
pub struct Script {
    tx: Sender<ToMain>,
    rx: Receiver<ToScript>,
}

/// The two ends of a new link.
pub fn link() -> (Main, Script) {
    let (to_script, from_main) = std::sync::mpsc::channel();
    let (to_main, from_script) = std::sync::mpsc::channel();
    (
        Main {
            tx: to_script,
            rx: from_script,
            running: false,
            calling: false,
            called: false,
            serving: false,
            step_asked: false,
        },
        Script { tx: to_main, rx: from_main },
    )
}

impl Main {
    /// Read what the script thread has said, without waiting.
    pub fn poll(&mut self) {
        loop {
            match self.rx.try_recv() {
                Ok(message) => self.take(message),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.gone();
                    break;
                }
            }
        }
    }

    /// Wait up to `limit` for the thread to say something, and read it and
    /// whatever followed.
    pub fn wait(&mut self, limit: Duration) {
        match self.rx.recv_timeout(limit) {
            Ok(message) => {
                self.take(message);
                self.poll();
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => self.gone(),
        }
    }

    fn take(&mut self, message: ToMain) {
        match message {
            ToMain::Ran => self.running = false,
            ToMain::Served => self.serving = false,
            ToMain::Called => {
                self.calling = false;
                self.called = true;
            }
            ToMain::Step => self.step_asked = true,
        }
    }

    /// The thread has ended -- a panic, its interpreter gone: nothing is
    /// running there, and nothing will answer.
    fn gone(&mut self) {
        self.running = false;
        // A callback being waited for is done with.
        self.called |= std::mem::take(&mut self.calling);
        self.serving = false;
        self.step_asked = false;
    }

    /// Whether a script is running on the thread: the scene's frames are
    /// then drawn when it steps.
    pub fn running(&self) -> bool {
        self.running
    }

    /// Whether the thread is free to take a script: none running, no
    /// callback being called, the python tab not being served.
    pub fn idle(&self) -> bool {
        !self.running && !self.calling && !self.serving
    }

    /// Whether a callback asked for has not returned, or its answer not
    /// been taken: `call` is the one to ask about it.
    pub fn calling(&self) -> bool {
        self.calling || self.called
    }

    /// Whether the python tab asked to be served has not been.
    pub fn serving(&self) -> bool {
        self.serving
    }

    /// Run a script on the thread.
    pub fn run(&mut self, path: String, source: String) {
        if self.tx.send(ToScript::Run { path, source }).is_ok() {
            self.running = true;
        }
    }

    /// Serve the python tab on the thread, asked once until it has been.
    pub fn console(&mut self) {
        if !self.serving && self.tx.send(ToScript::Console).is_ok() {
            self.serving = true;
        }
    }

    /// Whether a script's `step()` waits for a frame.
    pub fn step_asked(&self) -> bool {
        self.step_asked
    }

    /// The frame a step asked for is drawn.
    pub fn answer_step(&mut self, running: bool) {
        if std::mem::take(&mut self.step_asked) {
            let _ = self.tx.send(ToScript::Frame { running });
        }
    }

    /// Call a callback on the thread, asked once: true once it has returned,
    /// its answer taken.
    pub fn call(&mut self, before: bool, dt: crate::Float) -> bool {
        if !self.calling && !self.called {
            if self.tx.send(ToScript::Callback { before, dt }).is_err() {
                return true;
            }
            self.calling = true;
            // Most callbacks are quick: a short wait saves the frame a turn.
            self.wait(Duration::from_millis(2));
        } else {
            self.poll();
        }
        std::mem::take(&mut self.called)
    }
}

impl Script {
    /// The next thing asked, waiting for it; `None` once the main thread has
    /// let go of its end.
    pub fn next(&self) -> Option<ToScript> {
        self.rx.recv().ok()
    }

    /// Say something back.
    pub fn send(&self, message: ToMain) {
        let _ = self.tx.send(message);
    }
}
