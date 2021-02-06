use eframe::{egui, epi};
use rt_audio_disk_stream::AudioDiskStream;
use rtrb::{Consumer, Producer, RingBuffer};

use crate::{GuiToProcessMsg, ProcessToGuiMsg};

pub struct DemoPlayerApp {
    playing: bool,
    current_frame: usize,
    num_frames: usize,
    transport_control: TransportControl,

    to_player_tx: Producer<GuiToProcessMsg>,
    from_player_rx: Consumer<ProcessToGuiMsg>,

    frame_close_tx: Producer<()>,
    frame_close_rx: Option<Consumer<()>>,
}

impl DemoPlayerApp {
    pub fn new(
        mut to_player_tx: Producer<GuiToProcessMsg>,
        from_player_rx: Consumer<ProcessToGuiMsg>,
    ) -> Self {
        let mut test_client =
            AudioDiskStream::open_read("./test_files/wav_i24_mono.wav", 0, 2, true).unwrap();

        test_client.seek_to(0, 0).unwrap();
        test_client.block_until_ready().unwrap();

        let num_frames = test_client.info().num_frames;

        to_player_tx
            .push(GuiToProcessMsg::UseStream(test_client))
            .unwrap();

        to_player_tx
            .push(GuiToProcessMsg::SetLoop {
                start: 0,
                end: num_frames - 500000,
            })
            .unwrap();

        let (frame_close_tx, frame_close_rx) = RingBuffer::new(1).split();

        Self {
            playing: false,
            current_frame: 0,
            num_frames,
            transport_control: Default::default(),

            frame_close_tx,
            frame_close_rx: Some(frame_close_rx),

            to_player_tx,
            from_player_rx,
        }
    }
}

impl epi::App for DemoPlayerApp {
    fn name(&self) -> &str {
        "rt-audio-disk-stream demo player"
    }

    fn update(&mut self, ctx: &egui::CtxRef, frame: &mut epi::Frame<'_>) {
        if let Some(mut frame_close_rx) = self.frame_close_rx.take() {
            // Spawn thread that calls a repaint 60 times a second.

            let repaint_signal = frame.repaint_signal().clone();

            std::thread::spawn(move || {
                loop {
                    std::thread::sleep(std::time::Duration::from_secs_f64(1.0 / 60.0));

                    // Check if app has closed.
                    if let Ok(_) = frame_close_rx.pop() {
                        break;
                    }

                    repaint_signal.request_repaint();
                }
            });
        }

        while let Ok(msg) = self.from_player_rx.pop() {
            match msg {
                ProcessToGuiMsg::TransportPos(pos) => {
                    self.current_frame = pos;
                }
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::warn_if_debug_build(ui);

            let play_label = if self.playing { "||" } else { ">" };

            if ui.button(play_label).clicked {
                if self.playing {
                    self.playing = false;

                    self.to_player_tx.push(GuiToProcessMsg::Pause).unwrap();
                } else {
                    self.playing = true;

                    self.to_player_tx.push(GuiToProcessMsg::PlayResume).unwrap();
                }
            }

            self.transport_control
                .ui(ui, &mut self.current_frame, self.num_frames);
        });
    }
}

impl Drop for DemoPlayerApp {
    fn drop(&mut self) {
        self.frame_close_tx.push(()).unwrap();
    }
}

struct TransportControl {
    rail_stroke: egui::Stroke,
    handle_stroke: egui::Stroke,
}

impl Default for TransportControl {
    fn default() -> Self {
        Self {
            rail_stroke: egui::Stroke::new(1.0, egui::Color32::GRAY),
            handle_stroke: egui::Stroke::new(1.0, egui::Color32::WHITE),
        }
    }
}

impl TransportControl {
    const PADDING: f32 = 20.0;

    pub fn ui(&mut self, ui: &mut egui::Ui, value: &mut usize, max_value: usize) -> egui::Response {
        let (response, painter) =
            ui.allocate_painter(ui.available_size_before_wrap_finite(), egui::Sense::drag());
        let rect = response.rect;

        let mut shapes = vec![];

        let rail_y = rect.top() + 20.0;
        let start_x = rect.left() + Self::PADDING;
        let end_x = rect.right() - Self::PADDING;
        let rail_width = end_x - start_x;

        // Draw rail.
        shapes.push(egui::Shape::line_segment(
            [
                egui::Pos2::new(start_x, rail_y),
                egui::Pos2::new(end_x, rail_y),
            ],
            self.rail_stroke,
        ));

        if let Some(press_origin) = ui.input().mouse.press_origin {
            if press_origin.x >= start_x
                && press_origin.x <= end_x
                && press_origin.y >= rail_y - 10.0
                && press_origin.y <= rail_y + 10.0
            {
                if let Some(mouse_pos) = ui.input().mouse.pos {
                    let handle_x = mouse_pos.x - start_x;
                    *value = (((handle_x / rail_width) * max_value as f32).round() as isize)
                        .max(0)
                        .min(max_value as isize) as usize;
                }
            }
        }

        let handle_x = start_x + ((*value as f32 / max_value as f32) * rail_width);

        // Draw handle.
        shapes.push(egui::Shape::line_segment(
            [
                egui::Pos2::new(handle_x, rail_y - 10.0),
                egui::Pos2::new(handle_x, rail_y + 10.0),
            ],
            self.handle_stroke,
        ));

        painter.extend(shapes);

        response
    }
}