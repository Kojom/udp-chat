use anyhow::Result;
use eframe::{egui, App, Frame};
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::process;
use std::sync::{Arc, Mutex};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

struct ChatApp {
    messages: Arc<Mutex<Vec<String>>>,
    input: String,
    sender: Option<mpsc::Sender<String>>,
    client_id: u32,
}

impl Default for ChatApp {
    fn default() -> Self {
        Self {
            messages: Arc::new(Mutex::new(Vec::new())),
            input: String::new(),
            sender: None,
            client_id: process::id(),
        }
    }
}

impl App for ChatApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let available = ui.available_size();

            let input_height = 32.0;
            let scroll_height = available.y - input_height;

            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .stick_to_bottom(true)
                .max_height(scroll_height)
                .show(ui, |ui| {
                    let msgs = self.messages.lock().unwrap();
                    for msg in msgs.iter() {
                        egui::Frame::group(ui.style())
                            .fill(ui.visuals().extreme_bg_color)
                            .rounding(egui::Rounding::same(8.0))
                            .stroke(egui::Stroke::NONE)
                            .inner_margin(egui::Margin::same(6.0))
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new(msg).monospace().size(14.0),
                                );
                            });

                        ui.add_space(4.0);
                    }
                });

            ui.add_space(2.0);

            ui.horizontal(|ui| {
                let send_pressed = ui.text_edit_singleline(&mut self.input).lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter));

                if ui.button("Send").clicked() || send_pressed {
                    if let Some(sender) = &self.sender {
                        let msg = format!("{}:{}", self.client_id, self.input);
                        let _ = sender.try_send(msg);
                        {
                            let mut locked = self.messages.lock().unwrap();
                            locked.push(format!("Me: {}", self.input));
                        }
                        self.input.clear();
                    }
                }
            });
        });

        ctx.request_repaint();
    }
}

async fn create_broadcast_socket() -> Result<UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    #[cfg(target_family = "unix")]
    socket.set_reuse_port(true)?;
    socket.set_broadcast(true)?;
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 42069);
    socket.bind(&addr.into())?;

    let std_socket: std::net::UdpSocket = socket.into();

    std_socket.set_nonblocking(true)?;
    let udp_socket = UdpSocket::from_std(std_socket)?;

    Ok(udp_socket)
}

fn main() -> Result<()> {
    let (tx, mut rx) = mpsc::channel::<String>(32);
    let (gui_tx, gui_rx) = std::sync::mpsc::channel::<String>();

    let messages = Arc::new(Mutex::new(Vec::new()));
    let gui_messages = messages.clone();
    let client_id = process::id();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async move {
            let socket = create_broadcast_socket().await.unwrap();

            let std_socket = socket.into_std().unwrap();
            let std_socket_clone = std_socket.try_clone().unwrap();

            let send_socket = UdpSocket::from_std(std_socket).unwrap();
            let recv_socket = UdpSocket::from_std(std_socket_clone).unwrap();

            let recv_gui_tx = gui_tx.clone();

            tokio::spawn(async move {
                let mut buf = vec![0u8; 1024];
                loop {
                    match recv_socket.recv_from(&mut buf).await {
                        Ok((len, _addr)) => {
                            let msg = String::from_utf8_lossy(&buf[..len]).to_string();

                            if let Some((sender_id_str, text)) = msg.split_once(':') {
                                if let Ok(sender_id) = sender_id_str.parse::<u32>() {
                                    if sender_id == client_id {
                                        continue;
                                    }
                                    let display_msg = format!("User {}: {}", sender_id, text);
                                    let _ = recv_gui_tx.send(display_msg);
                                } else {
                                    let _ = recv_gui_tx.send(msg);
                                }
                            } else {
                                let _ = recv_gui_tx.send(msg);
                            }
                        }
                        Err(_) => continue,
                    }
                }
            });

            tokio::spawn(async move {
                while let Some(msg) = rx.recv().await {
                    let _ = send_socket
                        .send_to(msg.as_bytes(), "255.255.255.255:42069")
                        .await;
                }
            });

            loop {
                while let Ok(msg) = gui_rx.try_recv() {
                    let mut locked = messages.lock().unwrap();
                    locked.push(msg);
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(30)).await;
            }
        });
    });

    let app = ChatApp {
        messages: gui_messages,
        input: String::new(),
        sender: Some(tx),
        client_id,
    };
    let options = eframe::NativeOptions::default();

    let res = eframe::run_native("UDP Chat", options, Box::new(|_cc| Box::new(app)));
    if let Err(e) = res {
        eprintln!("GUI error: {:?}", e);
    }

    Ok(())
}
