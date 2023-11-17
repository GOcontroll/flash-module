use core::panic;
use std::{
	fs::File,
	fs,
	io::Read,
	env,
	process::{Command, exit},
	time::Duration,
	fmt::{Write, Display},
	thread,
	mem,
};

use spidev::{Spidev, SpidevOptions, SpiModeFlags, SpidevTransfer};

use inquire::Select;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle, ProgressState};

const DUMMY_MESSAGE: [u8;5] = [0;5];

const BOOTMESSAGE_LENGTH: usize = 46;
const BOOTMESSAGE_LENGTH_CHECK: usize = 61;

const SPI_FILE_FAULT: &str = "spidev does not exist";

const SLOT_PROMPT: &str = "Which slot to overwrite?";

const USAGE: &str = 
"Usage:
go-flash-module <slot> <file> [force]
or
go-flash-module
<>: must be present
[]: optional

examples:
go-flash-module 1 20-20-2-6-1-3-2.srec 1 //forced flash of outputmodule firmware to the module in slot 1
go-flash-module 8 20-10-1-5-2-0-0.srec   //checked flash of inputmodule firmware to the module in slot 8
go-flash-module                          //flash with the tui";

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct FirmwareVersion {
	firmware: [u8;7],
}

impl FirmwareVersion {
	/// create a FirmwareVersion from a filename for example 20-10-1-5-0-0-9.srec
	fn from_filename(name: String) -> Option<Self> {
		let mut firmware: [u8;7] = [0u8;7];
		if let Some(no_extension) = name.split(".").next() {
			let numbers = no_extension.split("-");

			for (i, num) in numbers.enumerate() {
				let part = firmware.get_mut(i)?;
				if let Ok(file_part) = u8::from_str_radix(num, 10) {
					*part = file_part;
				} else {
					return None;
				}
			}
		}
		Some(Self {
			firmware,
		})
	}

	/// get the software part of the firmware version
	fn get_software(&self) -> &[u8] {
		self.firmware.get(4..7).unwrap()
	}

	/// get the hardware part of the firmware version
	fn get_hardware(&self) -> &[u8] {
		self.firmware.get(0..4).unwrap()
	}

	/// get a string version of the firmware version like 20-10-1-5-0-0-9
	fn to_string(&self) -> String {
		format!("{}-{}-{}-{}-{}-{}-{}",self.firmware[0],self.firmware[1],self.firmware[2],self.firmware[3],self.firmware[4],self.firmware[5],self.firmware[6])
	}

	/// get a filename version of the firmware version like 20-10-1-5-0-0-9.srec
	fn to_filename(&self) -> String {
		format!("{}.srec",self.to_string())
	}
}

impl Display for FirmwareVersion {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f,"{}", self.to_filename())
	}
}

enum CommandArg {
	Scan,
	Update,
	Overwrite
}

impl Display for CommandArg {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", match self {
			CommandArg::Scan => "scan",
			CommandArg::Update => "update",
			CommandArg::Overwrite => "overwrite",
		})
	}
}

enum UpdateType {
	All,
	One(Module),
}

enum UploadError {
	FirmwareCorrupted(u8),
	FirmwareUntouched(u8),
}

#[repr(usize)]
#[derive(Copy,Clone)]
enum ControllerTypes {
	ModulineIV = 9,
	ModulineMini = 5,
	ModulineDisplay = 3,
}

#[derive(Debug)]
struct Module {
	slot: u8,
	spidev: Spidev,
	firmware: FirmwareVersion,
	manufacturer: u32,
	qr_front: u32,
	qr_back: u32,
}

impl Module {
	/// construct a new module at the given slot for the given controller type
	fn new(slot:u8, controller: &ControllerTypes) -> Option<Self> {
		//get the spidev
		let mut spidev = match controller { //get the Interrupt GPIO and the spidev
			ControllerTypes::ModulineIV => {
				match slot {
					0 => panic!("0 is not a valid module slot"),
					1 => {
						Spidev::new(File::open("/dev/spidev1.0").expect(SPI_FILE_FAULT))
					},
					2 => {
						Spidev::new(File::open("/dev/spidev1.1").expect(SPI_FILE_FAULT))
					},
					3 => {
						Spidev::new(File::open("/dev/spidev2.0").expect(SPI_FILE_FAULT))
					},
					4 => {
						Spidev::new(File::open("/dev/spidev2.1").expect(SPI_FILE_FAULT))
					},
					5 => {
						Spidev::new(File::open("/dev/spidev2.2").expect(SPI_FILE_FAULT))
					},
					6 => {
						Spidev::new(File::open("/dev/spidev2.3").expect(SPI_FILE_FAULT))
					},
					7 => {
						Spidev::new(File::open("/dev/spidev0.0").expect(SPI_FILE_FAULT))
					},
					8 => {
						Spidev::new(File::open("/dev/spidev0.1").expect(SPI_FILE_FAULT))
					},
					_ => panic!("The Moduline IV only has 8 slots"),
				}
			},
			ControllerTypes::ModulineMini => {
				match slot {
					0 => panic!("0 is not a valid module slot"),
					1 => {
						Spidev::new(File::open("/dev/spidev1.0").expect(SPI_FILE_FAULT))
					},
					2 => {
						Spidev::new(File::open("/dev/spidev1.1").expect(SPI_FILE_FAULT))
					},
					3 => {
						Spidev::new(File::open("/dev/spidev2.0").expect(SPI_FILE_FAULT))
					},
					4 => {
						Spidev::new(File::open("/dev/spidev2.1").expect(SPI_FILE_FAULT))
					},
					_ => panic!("The Moduline Mini only has 4 slots")
				}
			},
			ControllerTypes::ModulineDisplay => {
				match slot {
					0 => panic!("0 is not a valid module slot"),
					1 => {
						Spidev::new(File::open("/dev/spidev1.0").expect(SPI_FILE_FAULT))
					},
					2 => {
						Spidev::new(File::open("/dev/spidev1.1").expect(SPI_FILE_FAULT))
					},
					_ => panic!("The Moduline Screen only has 2 slots"),
				}
			}
		};
		spidev.configure(&SpidevOptions::new()
			.bits_per_word(8)
			.max_speed_hz(2_000_000)
			.mode(SpiModeFlags::SPI_MODE_0)
			.build()).unwrap();
		let module = Self {
			slot,
			spidev,
			firmware: FirmwareVersion { firmware: [0;7] },
			manufacturer: 0,
			qr_front: 0,
			qr_back: 0,
		};
		module.get_module_info()
	}
	
	/// get information from the module like firmware, manufacture, qr codes
	fn get_module_info(mut self) -> Option<Module> {
		let mut tx_buf = [0u8;BOOTMESSAGE_LENGTH+1];
		let mut rx_buf = [0u8;BOOTMESSAGE_LENGTH+1];

		match self.spidev.transfer(&mut SpidevTransfer::write(&DUMMY_MESSAGE)) {
			Ok(()) => (),
			Err(_) => return None,
		}
	
		self.reset_module(true);
	
		//give module time to reset
		thread::sleep(Duration::from_millis(200));
	
		self.reset_module(false);
	
		thread::sleep(Duration::from_millis(200));
	
		tx_buf[0] = 9;
		tx_buf[1] = (BOOTMESSAGE_LENGTH-1) as u8;
		tx_buf[2] = 9;
		tx_buf[BOOTMESSAGE_LENGTH-1] = calculate_checksum(&tx_buf, BOOTMESSAGE_LENGTH-1);
	
		match self.spidev.transfer(&mut SpidevTransfer::read_write(&tx_buf, &mut rx_buf)) {
			Ok(()) => (),
			Err(_) => {
				return None
			},		
		}
	
		if rx_buf[BOOTMESSAGE_LENGTH-1] != calculate_checksum(&rx_buf, BOOTMESSAGE_LENGTH-1) {
			return None
		} else if rx_buf[0] != 9 && rx_buf[2] != 9 {
			return None
		}
	
		self.firmware =  FirmwareVersion {
			firmware: clone_into_array(rx_buf.get(6..13).unwrap())
		};
		self.manufacturer = u32::from_be_bytes(clone_into_array(rx_buf.get(13..17).unwrap()));
		self.qr_front = u32::from_be_bytes(clone_into_array(rx_buf.get(17..21).unwrap()));
		self.qr_back = u32::from_be_bytes(clone_into_array(rx_buf.get(21..25).unwrap()));
		Some(self)
	}

	/// switch the reset gpio for the module to the given state
	fn reset_module(&self, state: bool) {
		if state {
			_=std::fs::write(format!("/sys/class/leds/ResetM-{}/brightness",self.slot), "255");
		} else {
			_=std::fs::write(format!("/sys/class/leds/ResetM-{}/brightness",self.slot), "0");
		}
	}

	/// overwrite the firmware on a module
	fn overwrite_module(&mut self, new_firmware: &FirmwareVersion, multi_progress: MultiProgress, style: ProgressStyle) -> Result<(), UploadError> {
		let mut tx_buf_escape = [0u8;BOOTMESSAGE_LENGTH_CHECK];
		let mut rx_buf_escape = [0u8;BOOTMESSAGE_LENGTH_CHECK];

		let mut tx_buf = [0u8;BOOTMESSAGE_LENGTH+1];
		let mut rx_buf = [0u8;BOOTMESSAGE_LENGTH+1];

		let mut firmware_file = match File::open(format!("/usr/module-firmware/{}",new_firmware.to_filename())) {
			Ok(file) => file,
			Err(_) => return Err(UploadError::FirmwareUntouched(self.slot)),
		};

		//upload
		//open and read the firmware file
		let mut firmware_content_string = String::with_capacity(firmware_file.metadata().unwrap().len() as usize + 10);
		match firmware_file.read_to_string(&mut firmware_content_string) {
			Ok(_) => (),
			Err(_) => return Err(UploadError::FirmwareUntouched(self.slot)),
		}

		let lines: Vec<&str> = firmware_content_string.split("\n").collect();
		let mut line_number: usize = 0;
		#[allow(unused_assignments)]
		let mut send_buffer_pointer: usize = 0;
		#[allow(unused_assignments)]
		let mut message_pointer: usize = 0;
		let mut message_type: u8 = 0;
		let mut firmware_line_check: usize = 0;
		let mut firmware_error_counter: u8 = 0;

		if lines.len() <= 1 {
			println!("error: Firmware file corrupt");
			return Err(UploadError::FirmwareUntouched(self.slot));
		}
		//wipe the old firmware and set the new software version no err_n_restart_services from this point on, errors lead to corrupt firmware.
		tx_buf[0] = 29;
		tx_buf[1] = (BOOTMESSAGE_LENGTH-1) as u8;
		tx_buf[2] = 29;
		let sw = new_firmware.get_software();
		tx_buf[6] = sw[0];
		tx_buf[7] = sw[1];
		tx_buf[8] = sw[2];

		tx_buf[BOOTMESSAGE_LENGTH-1] = calculate_checksum(&tx_buf, BOOTMESSAGE_LENGTH-1);

		match self.spidev.transfer(&mut SpidevTransfer::write(&tx_buf)) {
			Ok(()) => (),
			Err(_) => return Err(UploadError::FirmwareUntouched(self.slot)),
		}

		println!("wiping old firmware...");

		thread::sleep(Duration::from_millis(2500));

		let progress = multi_progress.add(ProgressBar::new(lines.len() as u64));
		progress.set_style(style);
		progress.set_message(format!("Uploading firmware to slot {}", self.slot));


		while message_type != 7 {
			message_type = u8::from_str_radix(lines[line_number].get(1..2).unwrap(), 16).unwrap();

			let line_length = u8::from_str_radix(lines[line_number].get(2..4).unwrap(), 16).unwrap();
			if message_type == 7 && firmware_line_check != line_number {
				tx_buf[0] = 49;
				tx_buf[1] = (BOOTMESSAGE_LENGTH-1) as u8;
				tx_buf[2] = 49;
				tx_buf[BOOTMESSAGE_LENGTH-1] = calculate_checksum(&tx_buf, BOOTMESSAGE_LENGTH-1);
				match self.spidev.transfer(&mut SpidevTransfer::read_write(&tx_buf, &mut rx_buf)) {
					Ok(()) => {
						if rx_buf[BOOTMESSAGE_LENGTH-1] == calculate_checksum(&rx_buf, BOOTMESSAGE_LENGTH-1) &&
						firmware_line_check == u16::from_be_bytes(clone_into_array(rx_buf.get(6..8).unwrap())) as usize &&
						rx_buf[8] == 1 {
							thread::sleep(Duration::from_millis(5));
						}
					},
					Err(_) => {
						firmware_error_counter += 1;
						mem::swap(&mut line_number, &mut firmware_line_check);
						message_type = 0;
						thread::sleep(Duration::from_millis(5));
						continue;
					},
				}
			}

			tx_buf[0] = 39;
			tx_buf[1] = (BOOTMESSAGE_LENGTH - 1 ) as u8;
			tx_buf[2] = 39;

			send_buffer_pointer = 6;
			tx_buf[send_buffer_pointer] = (line_number >> 8) as u8;
			send_buffer_pointer +=1;
			tx_buf[send_buffer_pointer] = line_number as u8;
			send_buffer_pointer +=1;
			tx_buf[send_buffer_pointer] = message_type;
			send_buffer_pointer +=1;

			message_pointer = 2;
			while message_pointer < ((line_length*2) + 2) as usize{
				tx_buf[send_buffer_pointer] = u8::from_str_radix(lines[line_number].get(message_pointer..message_pointer+2).unwrap(), 16).unwrap();
				send_buffer_pointer += 1;
				message_pointer += 2;
			}
			tx_buf[send_buffer_pointer] = u8::from_str_radix(lines[line_number].get(message_pointer..message_pointer+2).unwrap(), 16).unwrap();

			tx_buf[BOOTMESSAGE_LENGTH-1] = calculate_checksum(&tx_buf, BOOTMESSAGE_LENGTH-1);

			_=self.spidev.transfer(&mut SpidevTransfer::read_write(&tx_buf, &mut rx_buf));
			thread::sleep(Duration::from_millis(1));

			if rx_buf[BOOTMESSAGE_LENGTH-1] == calculate_checksum(&rx_buf, BOOTMESSAGE_LENGTH-1){
				if firmware_line_check == u16::from_be_bytes(clone_into_array(rx_buf.get(6..8).unwrap())) as usize {
					if rx_buf[8] == 1 {
						if firmware_error_counter & 0b1 > 0{
							std::mem::swap(&mut line_number, &mut firmware_line_check);
						} else {
							firmware_line_check = line_number;
						}

						if message_type == 7 {
							tx_buf_escape[0] = 49;
							tx_buf_escape[1] = (BOOTMESSAGE_LENGTH-1) as u8;
							tx_buf_escape[2] = 49;
							tx_buf_escape[BOOTMESSAGE_LENGTH-1] = calculate_checksum(&tx_buf_escape, BOOTMESSAGE_LENGTH-1);
							thread::sleep(Duration::from_millis(5));
							_=self.spidev.transfer(&mut SpidevTransfer::read_write(&tx_buf_escape, &mut rx_buf_escape));
							if !(rx_buf_escape[rx_buf_escape[1] as usize] == calculate_checksum(&rx_buf_escape, rx_buf_escape[1] as usize) && rx_buf_escape[6] == 20) {
								message_type = 0;
							} else {
								progress.inc(1);
							}
						} else {
							line_number += 1;
							firmware_error_counter = 0;
							progress.inc(1);
						}
					} else {
						mem::swap(&mut line_number, &mut firmware_line_check);
						message_type = 0;
						firmware_error_counter += 1;

						if firmware_error_counter > 10 {
							progress.abandon_with_message("Upload Failed");
							return Err(UploadError::FirmwareCorrupted(self.slot));
						}
					}
				} else {
					mem::swap(&mut line_number, &mut firmware_line_check);
					message_type = 0;
					firmware_error_counter += 1;

					if firmware_error_counter > 10 {
						progress.abandon_with_message("Upload Failed");
						return Err(UploadError::FirmwareCorrupted(self.slot));
					}
				}
			} else {
				mem::swap(&mut line_number, &mut firmware_line_check);
				message_type = 0;
				firmware_error_counter += 1;

				if firmware_error_counter > 10 {
					progress.abandon_with_message("Upload Failed");
					return Err(UploadError::FirmwareCorrupted(self.slot));
				}
			}
		}

		_ = progress.set_message("Upload finished");
		_ = progress.finish();
		self.cancel_firmware_upload(&mut tx_buf);
		return Ok(());
	}

	/// update a module, checking for new matching firmwares in the firmwares parameter
	fn update_module(mut self, firmwares: &Vec<FirmwareVersion>, multi_progress: MultiProgress, style: ProgressStyle) -> Result<Result<Module, Module>, UploadError> {
		if let Some((index,_junk)) = firmwares.iter().enumerate()
			.filter(|(_i,available)| available.get_hardware() == self.firmware.get_hardware())//filter out incorrect hardware versions
			.filter(|(_i,available)| (available.get_software() > self.firmware.get_software() || self.firmware.get_software() == &[255u8,255,255]) && available.get_software() != &[255u8,255,255])//filter out wrong software versions
			.map(|(i,available)| (i,available.get_software()))//turn them all into software versions
			.min(){ //get the highest firmware version for some reason min gives that instead of max?
				println!("updating slot {} from {} to {}", self.slot, self.firmware.to_string(), firmwares.get(index).unwrap().to_string());
				match self.overwrite_module(firmwares.get(index).unwrap(),multi_progress, style) {
					Ok(()) => {
						self.firmware = firmwares.get(index).unwrap().clone();
						return Ok(Ok(self))}
					,
					Err(err) => return Err(err),
				}
		} else {
			Ok(Err(self))
		}
	}

	/// cancel the firmware upload of the module bringing the module into operational state
	fn cancel_firmware_upload(&mut self, tx_buf: &mut [u8]) {
		tx_buf[0] = 19;
		tx_buf[1] = (BOOTMESSAGE_LENGTH-1) as u8;
		tx_buf[2] = 19;
		tx_buf[BOOTMESSAGE_LENGTH-1] = calculate_checksum(&tx_buf, BOOTMESSAGE_LENGTH-1);
		_=self.spidev.transfer(&mut SpidevTransfer::write(&tx_buf));
	}
}

impl Display for Module {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", match self.firmware.get_hardware().get(1).unwrap() {
			10 => match self.firmware.get_hardware().get(2).unwrap() {
				1 => format!("slot {}: 6 Channel Input module",self.slot),
				2 => format!("slot {}: 10 Channel Input module", self.slot),
				3 => format!("slot {}: 4-20mA Input module", self.slot),
				_ => format!("slot {}: unknown: {}",self.slot,self.firmware.to_string()),
			},
			20 => match self.firmware.get_hardware().get(2).unwrap() {
				1 => format!("slot {}: 2 Channel Output module", self.slot),
				2 => format!("slot {}: 6 Channel Output module", self.slot),
				3 => format!("slot {}: 10 Channel Output module", self.slot),
				_ => format!("slot {}: unknown: {}", self.slot, self.firmware.to_string()),
			},
			30 => match self.firmware.get_hardware().get(2).unwrap() {
				3 => format!("slot {}: ANLEG IR module", self.slot),
				_ => format!("slot {}: unknown: {}", self.slot, self.firmware.to_string()),
			},
			_ => format!("slot {}: unknown: {}", self.slot, self.firmware.to_string()),
		})
	}
}

/// error out and restart nodered and go-simulink if required
fn err_n_restart_services(nodered: bool, simulink: bool) -> ! {
	if nodered {
		_ = Command::new("systemctl")
			.arg("start")
			.arg("nodered")
			.spawn();
	}

	if simulink {
		_ = Command::new("systemctl")
			.arg("start")
			.arg("go-simulink")
			.spawn();
	}
	exit(-1);
}

/// exit with a success code and restart the nodered and go-simulink services if required
fn success(nodered: bool, simulink: bool) -> ! {
	if nodered {
		_ = Command::new("systemctl")
			.arg("start")
			.arg("nodered")
			.spawn();
	}

	if simulink {
		_ = Command::new("systemctl")
			.arg("start")
			.arg("go-simulink")
			.spawn();
	}
	exit(0);
}

/// error out without restarting any services
fn err_n_die(message: &str) -> ! {
	println!("{}",message);
	exit(-1);
}

/// calculate an spi messages checksum
fn calculate_checksum(message: &[u8], length: usize) -> u8 {
	let mut checksum: u8 = 0;
	for val in message.get(0..length).unwrap(){
		checksum=checksum.wrapping_add(*val);
	}
	checksum
}

/// turn a slice into a sized array to perform ::from_bytes() operations on
fn clone_into_array<A, T>(slice: &[T]) -> A
where
	A: Default + AsMut<[T]>,
	T: Clone,
{
	let mut a = A::default();
	<A as AsMut<[T]>>::as_mut(&mut a).clone_from_slice(slice);
	a
}

/// get the current modules in the controller
fn get_modules(controller: &ControllerTypes) -> Vec<Module> {
	let mut modules = Vec::with_capacity(8);
	thread::scope(|t| {
		let mut scan_threads = Vec::with_capacity(8);
		let controller = controller.clone();
		for i in 1..controller as usize {
			scan_threads.push(t.spawn(move || {
				Module::new(i as u8, &controller)
			}))
		}
		for thread in scan_threads {
			if let Ok(Some(module)) = thread.join() {
				modules.push(module);
			}
	}
	});
	modules

}

/// get the modules in the controller and save them
fn get_modules_and_save(controller: &ControllerTypes) -> Vec<Module> {
	let modules = get_modules(controller);
	let mut modules_out: Vec<Option<Module>> = match &controller {
		ControllerTypes::ModulineDisplay => vec![None,None],
		ControllerTypes::ModulineIV => vec![None,None,None,None,None,None,None,None],
		ControllerTypes::ModulineMini => vec![None,None,None,None],
	};
	for module in modules {
		let slot = module.slot;
		modules_out[(slot -1) as usize] = Some(module);
	}
	save_modules(modules_out,&controller)
}

/// save all the modules to modules to /usr/module-firmware/modules.txt, None elements will be removed from the file
fn save_modules(modules: Vec<Option<Module>>, controller: &ControllerTypes) -> Vec<Module> {
	let modules_string = if let Ok(contents) = std::fs::read_to_string("/usr/module-firmware/modules.txt") {
		contents
	} else { //if the file doesn't exist, generate a new template
		match controller {
			ControllerTypes::ModulineIV => String::from(":::::::
:::::::
:::::::
:::::::"),
			ControllerTypes::ModulineMini => String::from(":::
:::
:::
:::"),
			ControllerTypes::ModulineDisplay => String::from(":
:
:
:"),
		}
	};
	let mut lines: Vec<String> = modules_string.split("\n").map(|element| element.to_owned()).collect();
	let mut firmwares: Vec<String> = lines.get_mut(0).unwrap().split(":").map(|element| element.to_owned()).collect();
	let mut manufactures: Vec<String> = lines.get_mut(1).unwrap().split(":").map(|element| element.to_owned()).collect();
	let mut front_qrs: Vec<String> = lines.get_mut(2).unwrap().split(":").map(|element| element.to_owned()).collect();
	let mut rear_qrs: Vec<String> = lines.get_mut(3).unwrap().split(":").map(|element| element.to_owned()).collect();

	for (i,module) in modules.iter().enumerate() {
		if let Some(module) = module {
			*firmwares.get_mut((module.slot-1) as usize).unwrap() = module.firmware.to_string();
			*manufactures.get_mut((module.slot-1) as usize).unwrap() = format!("{}", module.manufacturer);
			*front_qrs.get_mut((module.slot-1) as usize).unwrap() = format!("{}",module.qr_front);
			*rear_qrs.get_mut((module.slot-1) as usize).unwrap() = format!("{}",module.qr_back);
		} else {
			*firmwares.get_mut(i).unwrap() = "".to_string();
			*manufactures.get_mut(i).unwrap() = "".to_string();
			*front_qrs.get_mut(i).unwrap() = "".to_string();
			*rear_qrs.get_mut(i).unwrap() = "".to_string();
		}
	}
	lines[0] = firmwares.join(":");
	lines[1] = manufactures.join(":");
	lines[2] = front_qrs.join(":");
	lines[3] = rear_qrs.join(":");

	_ = std::fs::write("/usr/module-firmware/modules.txt", lines.join("\n"));
	modules.into_iter().flatten().collect()
}

fn main() {
	//get the controller hardware
	let hardware_string= fs::read_to_string("/sys/firmware/devicetree/base/hardware").expect("Your controller does not support this feature");

	let controller = if hardware_string.contains("Moduline IV") {
		ControllerTypes::ModulineIV
	} else if hardware_string.contains("Moduline Mini") {
		ControllerTypes::ModulineMini
	} else if hardware_string.contains("Moduline Screen") {
		ControllerTypes::ModulineDisplay
	} else {
		panic!("{} does not exist",hardware_string);
	};

	//get all the firmwares
	let available_firmwares: Vec<FirmwareVersion> = fs::read_dir("/usr/module-firmware/").unwrap() // get the files in module-firmware
	.map(|file| file.unwrap().file_name().to_str().unwrap().to_string()) //turn them into strings
	.filter(|file_name| file_name.ends_with(".srec")) //keep only the srec files
	.map(|firmware| FirmwareVersion::from_filename(firmware).unwrap())//turn them into FirmwareVersion Structs
	.collect(); //collect them into a vector

	let command = if let Some(arg) = env::args().nth(1) {
		match arg.as_str() {
			"scan" => CommandArg::Scan,
			"update" => CommandArg::Update,
			"overwrite" => CommandArg::Overwrite,
			_ => {
				eprintln!("Invalid command entered {}\n{}",arg, USAGE);
				exit(-1);
			}
		}
	} else {
		Select::new("What do you want to do?", vec![CommandArg::Scan, CommandArg::Update, CommandArg::Overwrite]).prompt().unwrap()
	};

	//stop services potentially trying to use the module
	let output = Command::new("systemctl")
		.arg("is-active")
		.arg("nodered")
		.output().unwrap().stdout;

	let nodered = !String::from_utf8_lossy(&output).to_owned().contains("in");

	let output =Command::new("systemctl")
		.arg("is-active")
		.arg("go-simulink")
		.output().unwrap().stdout;

	let simulink = !String::from_utf8_lossy(&output).to_owned().contains("in");

	if nodered {
		_ = Command::new("systemctl")
			.arg("stop")
			.arg("nodered")
			.status();
	}

	if simulink {
		_ = Command::new("systemctl")
			.arg("stop")
			.arg("go-simulink")
			.status();
	}
	
	match command {
		CommandArg::Scan => {
			let modules = get_modules_and_save(&controller);
			println!("found modules:");
			for module in &modules {
				println!("slot {}: {}", module.slot, module.firmware.to_string());
			}
			success(nodered, simulink);
		},


		CommandArg::Update => {
			let multi_progress = MultiProgress::new();
			let style = ProgressStyle::with_template(
				"[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} ({eta})",
			)
			.unwrap()
			.progress_chars("##-")
			.with_key("eta", |state: &ProgressState, w: &mut dyn Write| write!(w, "{:.1}s",state.eta().as_secs_f64()).unwrap());
			//find the update type
			let update = if let Some(arg) = env::args().nth(2) {
				match arg.as_str() {
					"all" => UpdateType::All,
					_ => if let Ok(slot) = u8::from_str_radix(&arg, 10) {
						if slot < controller as u8 || slot >= 1 {
							UpdateType::One(Module::new(slot, &controller).expect(format!("Couldn't find a module in slot {}", slot).as_str()))
						} else {
							println!("{}", USAGE);
							err_n_restart_services(nodered, simulink);
						}
					} else {
						println!("{}", USAGE);
						err_n_restart_services(nodered, simulink);
					}
				}
			} else {
				match Select::new("Update one module or all?", vec!["all", "one"]).prompt().unwrap() {
					"all" => UpdateType::All,
					"one" => {
						let modules = get_modules_and_save(&controller);
						if modules.len() > 0 {
							UpdateType::One(match Select::new("select a module to update", modules).with_page_size(8).prompt() {
								Ok(module) => module,
								Err(_) => {
									err_n_restart_services(nodered, simulink);
								}
							})
						} else {
							println!("No modules found in the controller.");
							err_n_restart_services(nodered, simulink);
						}
					}
					_ => {
						panic!("You shouldn't be here, turn back to whence you came");
					}
				}
			};
			//execute the update type
			match update {
				UpdateType::All => {
					let modules = get_modules_and_save(&controller);
					let mut upload_results = Vec::with_capacity(modules.len());
					let mut new_modules = Vec::with_capacity(modules.len());
					let mut firmware_corrupted = false;
					thread::scope(|t|{
						let mut threads = Vec::with_capacity(modules.len());
						for module in modules {
							threads.push(t.spawn(|| {
								module.update_module(&available_firmwares, multi_progress.clone(), style.clone())
							}))
						}
						for thread in threads {
							upload_results.push(thread.join().unwrap())
						}
					});
					for result in upload_results {
						match result {
							Ok(Ok(module)) => { //module updated
								new_modules.push(Some(module))
							},
							Err(err) => match err {
								UploadError::FirmwareCorrupted(slot) => {
									println!("Update failed, firmware is corrupted on slot {}",slot);
									firmware_corrupted = true;
								},
								UploadError::FirmwareUntouched(slot) => {
									println!("Update failed on slot {}", slot);
								}
							},
							Ok(Err(_)) => (), //no new firmwares available
						}
					}
					if new_modules.len() > 0 {
						println!("Succesfully updated:");
						for module in &new_modules {
							println!("slot {} to {}", module.as_ref().unwrap().slot, module.as_ref().unwrap().firmware.to_string());
						}
					} else if !firmware_corrupted {
						println!("No updates found for the modules in this controller.");
					}
					save_modules(new_modules, &controller);
					if firmware_corrupted {
						err_n_die("could not restart nodered and go-simulink services due to corrupted firmware.");	
					}
					
					success(nodered, simulink); 
				}
				UpdateType::One(module) => {
					match module.update_module(&available_firmwares, multi_progress, style) {
						Ok(Ok(module)) => {
							println!("Succesfully updated slot {} to {}", module.slot,module.firmware.to_string());
							save_modules(vec![Some(module)], &controller);
							success(nodered, simulink);
						},
						Err(err) => match err {
							UploadError::FirmwareCorrupted(slot) => {
								err_n_die(format!("Update failed, firmware is corrupted on slot {}", slot).as_str());
							},
							UploadError::FirmwareUntouched(slot) => {
								println!("Update failed on slot {}", slot);
								err_n_restart_services(nodered, simulink);
							}
						},
						Ok(Err(module)) => {
							println!("Update failed, no update available for slot {}: {}", module.slot, module.firmware.to_string());
							err_n_restart_services(nodered, simulink);
						}
					}
				}
				
			}
		},


		CommandArg::Overwrite => {
			//make the progress bar
			let multi_progress = MultiProgress::new();
			let style = ProgressStyle::with_template(
				"[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} ({eta})", // display time spent a bar of 40 characters in cyan/blue colour display progress as a number and the eta
			)
			.unwrap()
			.progress_chars("##-")
			.with_key("eta", |state: &ProgressState, w: &mut dyn Write| write!(w, "{:.1}s",state.eta().as_secs_f64()).unwrap());

			let mut module = if let Some(arg) = env::args().nth(2) {
				if let Ok(slot) = u8::from_str_radix(arg.as_str(), 10) {
					if let Some(module) = Module::new(slot, &controller) {
						module
					} else {
						println!("No module present in that slot");
						err_n_restart_services(nodered, simulink);
					}
				} else {
					println!("Invalid slot entered\n{}", USAGE);
					err_n_restart_services(nodered, simulink);
				}
			} else {
				let modules = get_modules_and_save(&controller);
				if modules.len() > 0 {
					Select::new(SLOT_PROMPT, modules).with_page_size(8).prompt().unwrap()
				} else {
					println!("No modules found in the controller.");
					err_n_restart_services(nodered, simulink);
				}
			};

			let new_firmware = if let Some(arg) = env::args().nth(3) {
				if let Some(firmware) = FirmwareVersion::from_filename(arg.clone()) {
					if available_firmwares.contains(&firmware){
						firmware
					} else {
						println!("/usr/module-firmware/{} does not exist",arg);
						err_n_restart_services(nodered, simulink);
					}
				} else {
					println!("Invalid firmware entered\n{}", USAGE);
					err_n_restart_services(nodered, simulink);
				}
			} else {
				let valid_firmwares: Vec<&FirmwareVersion> = available_firmwares.iter()
					.filter(|firmware| firmware.get_hardware() == module.firmware.get_hardware())
					.collect();
				if valid_firmwares.len() > 0 {
					Select::new("Which firmware to upload?", valid_firmwares).prompt().unwrap().clone()
				} else {
					println!("No firmware(s) found for this module.");
					err_n_restart_services(nodered, simulink);
				}
			};
			match module.overwrite_module(&new_firmware, multi_progress, style) {
				Ok(()) => {
					println!("succesfully updated slot {} from {} to {}", module.slot, module.firmware.to_string(), new_firmware.to_string());
					module.firmware = new_firmware;
					save_modules(vec![Some(module)], &controller);
					success(nodered, simulink);
				}
				Err(err) => {
					match err {
						UploadError::FirmwareCorrupted(slot) => {
							err_n_die(format!("Update failed, firmware is corrupted on slot {}", slot).as_str());
						},
						UploadError::FirmwareUntouched(slot) => {
							println!("Update failed on slot {}", slot);
							err_n_restart_services(nodered, simulink);
						}
					}	
				}
			}
		}
		
	}
}