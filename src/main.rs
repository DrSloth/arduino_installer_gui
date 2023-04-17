//! A small gui to install binaries to an Arduino Board

use eframe::egui;
use egui::{FontFamily, FontId, TextStyle};
use std::{
    borrow::Cow,
    io,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use rfd::FileDialog;
use serialport::SerialPortInfo;

/// The text styles applied to the shown text
const TEXT_STYLE: [(TextStyle, FontId); 5] = [
    (
        TextStyle::Heading,
        FontId::new(34.0, FontFamily::Proportional),
    ),
    (TextStyle::Body, FontId::new(27.0, FontFamily::Proportional)),
    (
        TextStyle::Monospace,
        FontId::new(24.0, FontFamily::Proportional),
    ),
    (
        TextStyle::Button,
        FontId::new(23.0, FontFamily::Proportional),
    ),
    (
        TextStyle::Small,
        FontId::new(20.0, FontFamily::Proportional),
    ),
];

fn main() {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "My egui App",
        native_options,
        Box::new(|cc| Box::new(ArduinoInstallerGui::new(cc))),
    )
    .unwrap_or_else(|e| panic!("Program failed: {}", e));
}

/// GUI Program State.
#[derive(Default)]
struct ArduinoInstallerGui {
    /// The file path the user selected of the file that should be installed.
    file_path: Option<PathBuf>,
    /// The selected board, which the program should be installed on.
    selected_board: ArduinoBoard,
    /// The selected port over which the board is connected.
    selected_port: Option<SerialPortInfo>,
    /// All available ports
    available_ports: Vec<SerialPortInfo>,
    /// The last error that happened when scanning the ports.
    port_scan_error: Option<String>,
    /// The general last error that happened.
    general_error: Option<Cow<'static, str>>,
    /// The output of the issued command.
    output: Option<String>,
    /// The command issed to install the program.
    used_command: Option<String>,
}

impl ArduinoInstallerGui {
    /// Create a new instance of the gui state.
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut styles = cc.egui_ctx.style().as_ref().clone();
        styles.text_styles = TEXT_STYLE.into();
        cc.egui_ctx.set_style(styles);

        let mut me = Self::default();
        portscan(&mut me.available_ports, &mut me.port_scan_error);
        me
    }
}

/// Scan for available ports
fn portscan(available_ports: &mut Vec<SerialPortInfo>, port_scan_error: &mut Option<String>) {
    match serialport::available_ports() {
        Ok(ports) => {
            *available_ports = ports;
            *port_scan_error = None;
        }
        Err(e) => {
            *available_ports = Vec::new();
            *port_scan_error = Some(format!("ERROR: {}", e));
        }
    }
}

impl eframe::App for ArduinoInstallerGui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.visuals_mut().override_text_color = Some(egui::Color32::WHITE);
            ui.heading("Arduino Installer gui");
            ui.horizontal(|ui| {
                ui.label("File: ");
                if let Some(ref path) = self.file_path {
                    ui.label(path.to_string_lossy().as_ref());
                }
                if ui.button("Choose a file").clicked() {
                    let file = FileDialog::new()
                        .add_filter("elf file", &["elf"])
                        .pick_file();
                    self.file_path = file;
                }
            });

            ui.horizontal(|ui| {
                ui.label("Select board: ");
                egui::ComboBox::from_id_source("Boards")
                    .selected_text(format!("{:?}", self.selected_board))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.selected_board,
                            ArduinoBoard::ArduinoUno,
                            "Arduino Uno",
                        );
                    });
            });

            ui.horizontal(|ui| {
                if ui.button("Rescan").clicked() {
                    portscan(&mut self.available_ports, &mut self.port_scan_error);
                } else {
                    let lbl = ui.label("Available Ports: ");
                    egui::ComboBox::from_id_source("Ports")
                        .selected_text(format!("{:?}", self.selected_port))
                        .wrap(false)
                        .width(lbl.rect.width().mul_add(-1.2, ui.available_width()))
                        .show_ui(ui, |ui| {
                            for info in self.available_ports.iter_mut() {
                                ui.selectable_value(
                                    &mut self.selected_port,
                                    Some(info.clone()),
                                    format!("{:?}: {}", info.port_type, info.port_name),
                                );
                            }
                        });
                }
            });

            ui.scope(|ui| {
                ui.visuals_mut().override_text_color = Some(egui::Color32::RED);
                if let Some(ref s) = self.port_scan_error {
                    ui.label(s);
                }

                if let Some(ref s) = self.general_error {
                    ui.label(s.as_ref());
                }
            });

            if ui.button("Flash device!").clicked() {
                match (&self.file_path, &self.selected_port) {
                    (&Some(ref path), &Some(ref port)) => {
                        let (used_command, res) = avrdude(self.selected_board.spec(), port, path);
                        self.output = Some(format!(
                            "Flashing: {:?}",
                            res.map(|out| String::from_utf8(out.stdout)),
                        ));
                        self.used_command = Some(used_command);
                    }
                    (&None, &None | &Some(_)) => {
                        self.general_error = Some("Error: no file selected".into());
                    }
                    (&Some(_), &None) => {
                        self.general_error = Some("Error: No port selected".into());
                    }
                }
            }

            if let Some(ref cmd) = self.used_command {
                ui.label(cmd);
            }

            if let Some(ref out) = self.output {
                ui.label(out);
            }
        });
    }
}

/// Enumeration of all supported Arduino boards
#[derive(Debug, Default, PartialEq, Clone, Copy)]
enum ArduinoBoard {
    /// The Arduino Uno
    #[default]
    ArduinoUno,
}

impl ArduinoBoard {
    /// The specification required to install a program to the board.
    fn spec(self) -> BoardSpec {
        match self {
            Self::ArduinoUno => BoardSpec {
                programmer: "arduino",
                partno: "atmega328p",
                do_chip_erase: true,
            },
        }
    }
}

/// A specification used to install a program to board (passed to avrdude).
#[derive(Debug, Clone)]
struct BoardSpec {
    /// The name of the onboard programmer.
    programmer: &'static str,
    /// The name of the chip the program should be installed to.
    partno: &'static str,
    /// Wether the chip should be whiped before installing.
    do_chip_erase: bool,
}

/// Call avrdude with the given spec to flash the given program to the device connected on the given
/// serial port.
fn avrdude(
    spec: BoardSpec,
    port: &SerialPortInfo,
    program_to_flash: &Path,
) -> (String, io::Result<Output>) {
    let mut cmd = Command::new("avrdude");
    cmd.arg("-c")
        .arg(spec.programmer)
        .arg("-p")
        .arg(spec.partno)
        .arg("-P")
        .arg(&port.port_name)
        .arg("-D")
        .arg("-U")
        .arg(&format!("flash:w:{}", program_to_flash.display()));

    if spec.do_chip_erase {
        cmd.arg("-e");
    }

    let used_command = format!("CMD: {:?}", cmd);

    (used_command, cmd.output())
}
