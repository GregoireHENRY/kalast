use crate::util::*;

use chrono::{Duration, NaiveTime, Utc};

pub struct Time {
    pub(crate) iteration: usize,
    pub(crate) elapsed_time: usize,
    pub(crate) time_step: usize,
    pub(crate) time_start: usize,
    pub(crate) real_time: NaiveTime,
    pub(crate) used_time_step: usize,
    pub(crate) file_row: Option<usize>,
    pub(crate) last_debug_time: Option<Float>,
}

impl Time {
    pub fn new() -> Self {
        Self {
            iteration: 0,
            elapsed_time: 0,
            time_step: 0,
            time_start: 0,
            real_time: Utc::now().time(),
            used_time_step: 0,
            file_row: None,
            last_debug_time: None,
        }
    }

    pub fn with_time_step(self, step: usize) -> Self {
        Self {
            iteration: self.iteration,
            elapsed_time: self.elapsed_time,
            time_step: step,
            time_start: self.time_start,
            real_time: self.real_time,
            used_time_step: self.used_time_step,
            file_row: self.file_row,
            last_debug_time: None,
        }
    }

    pub fn with_time_start(self, start: usize) -> Self {
        Self {
            iteration: self.iteration,
            elapsed_time: self.elapsed_time,
            time_step: self.time_step,
            time_start: start,
            real_time: self.real_time,
            used_time_step: self.used_time_step,
            file_row: self.file_row,
            last_debug_time: None,
        }
    }

    pub fn iteration(&self) -> usize {
        self.iteration
    }

    pub fn real_time(&self) -> &NaiveTime {
        &self.real_time
    }

    pub fn elapsed(&self) -> Duration {
        Duration::seconds(self.elapsed_time as _)
    }

    pub fn elapsed_seconds(&self) -> usize {
        self.elapsed().num_seconds() as _
    }

    pub fn elapsed_seconds_from_start(&self) -> usize {
        self.elapsed_seconds() + self.time_start
    }

    pub fn jd(&self) -> Float {
        self.elapsed_seconds_from_start() as Float / DAY as Float
    }

    pub fn time_step(&self) -> usize {
        self.time_step
    }

    pub fn used_time_step(&self) -> usize {
        self.used_time_step
    }

    pub fn set_time_step(&mut self, time_step: usize) {
        self.time_step = time_step;
    }

    pub fn next_iteration(&mut self) -> (usize, usize, usize) {
        self.iteration += 1;
        self.elapsed_time += self.time_step;
        self.used_time_step = self.time_step;
        (self.iteration, self.elapsed_time, self.time_step)
    }

    pub fn is_first_it(&self) -> bool {
        self.iteration == 0
    }
}
