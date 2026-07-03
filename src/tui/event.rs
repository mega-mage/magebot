use std::time::Duration;
use crossterm::event::{self, Event, KeyEvent, KeyEventKind, MouseEvent};
use crate::ipc::IpcMessage;

pub enum TuiEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Ipc(IpcMessage),
    Disconnected,
    Tick,
}

pub fn spawn_event_reader(tx: tokio::sync::mpsc::UnboundedSender<TuiEvent>) {
    tokio::task::spawn_blocking(move || {
        loop {
            match event::poll(Duration::from_millis(50)) {
                Ok(true) => match event::read() {
                    Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                        if tx.send(TuiEvent::Key(key)).is_err() {
                            break;
                        }
                    }
                    Ok(Event::Mouse(mouse)) => {
                        if tx.send(TuiEvent::Mouse(mouse)).is_err() {
                            break;
                        }
                    }
                    _ => {}
                },
                Ok(false) => {}
                Err(_) => break,
            }
        }
    });
}
