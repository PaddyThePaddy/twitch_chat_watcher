use std::{
    io::Cursor,
    sync::{
        mpsc::{channel, SendError, Sender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use rodio::{Decoder, OutputStream, Sink};

#[derive(Clone, Debug)]
pub struct AlertPlayer {
    volume: Arc<Mutex<f32>>,
    _worker_handler: Arc<JoinHandle<()>>,
    tx: Sender<f32>,
}

//unsafe impl std::marker::Send for AlertPlayer {}

impl Default for AlertPlayer {
    fn default() -> Self {
        let (tx, rx) = channel();
        let worker = thread::spawn(move || {
            //Sound Effect by UNIVERSFIELD from Pixabay
            let data = include_bytes!("../assets/new-notification-sound-effect-138807.mp3");
            let (_stream, _stream_handle) = OutputStream::try_default().unwrap();
            let sink = Sink::try_new(&_stream_handle).unwrap();
            let mut last_time_played = Instant::now() - Duration::from_secs(10);
            while let Ok(volume) = rx.recv() {
                if Instant::now() - last_time_played < Duration::from_secs(10) {
                    continue;
                }
                let source = Decoder::new(Cursor::new(data)).unwrap();
                sink.set_volume(volume);
                sink.append(source);
                last_time_played = Instant::now();
            }
        });
        Self {
            volume: Arc::new(Mutex::new(1.0)),
            _worker_handler: Arc::new(worker),
            tx,
        }
    }
}

impl AlertPlayer {
    pub fn play(&self) -> Result<(), SendError<f32>> {
        self.tx.send(*self.volume.lock().unwrap())
    }

    pub fn set_volume(&mut self, v: f32) {
        *self.volume.lock().unwrap() = v;
    }

    pub fn volume(&self) -> f32 {
        *self.volume.lock().unwrap()
    }
}
