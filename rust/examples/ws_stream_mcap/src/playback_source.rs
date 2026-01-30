use anyhow::Result;
use foxglove::websocket::PlaybackStatus;
use std::time::Instant;

pub trait PlaybackSource {
    fn time_bounds(&self) -> (u64, u64);
    fn set_playback_speed(&mut self, speed: f32);
    fn play(&mut self);
    fn pause(&mut self);
    fn status(&self) -> PlaybackStatus;
    fn seek(&mut self, log_time: u64) -> Result<()>;
    fn next_wakeup(&mut self) -> Result<Option<Instant>>;
    fn flush_since_last(&mut self) -> Result<()>;
    fn should_broadcast_time(&mut self) -> Option<u64>;
    fn current_time(&self) -> u64;
    fn playback_speed(&self) -> f32;
}
