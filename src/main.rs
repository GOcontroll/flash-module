use std::{
	fs::File,
	fs,
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

use tokio_gpiod::{Chip, Input, Options, Lines, EdgeDetect};

const DUMMY_MESSAGE: [u8;5] = [0;5];

const BOOTMESSAGE_LENGTH: usize = 46;
const BOOTMESSAGE_LENGTH_CHECK: usize = 61;

const SPI_FILE_FAULT: &str = "spidev does not exist";

const SLOT_PROMPT: &str = "Which slot to overwrite?";

const USAGE: &str = 
"Usage:
go-modules <command> [subcommands]
or
go-modules

commands:
scan							Scan the modules in the controller
update <all/slot#>				In case of all, try to update all modules, in case of a slot number, try to update that slot specifically
overwrite <slot> <firmware>		Overwrite the firmware in <slot> with <firmware>

examples:
go-modules										Use with the tui (recommended)
go-modules scan									Scan all modules in the controller
go-modules update all							Try to update all modules in the controller
go-modules update 1								Try to update the module in slot 1
go-modules overwrite 1 20-10-1-5-0-0-9.srec		Forcefully overwrite the module in slot 1 with 20-10-1-5-0-0-9.srec (can be used to downgrade modules)";

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct FirmwareVersion {
	firmware: [u8;7],
}

impl FirmwareVersion {
	/// create a FirmwareVersion from a filename for example 20-10-1-5-0-0-9.srec
	fn from_filename(name: String) -> Option<Self> {
		let mut firmware: [u8;7] = [0u8;7];
		if let Some(no_extension) = name.split('.').next() {
			let numbers = no_extension.split('-');

			for (i, num) in numbers.enumerate() {
				let part = firmware.get_mut(i)?;
				if let Ok(file_part) = num.parse::<u8>() {
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
	fn as_string(&self) -> String {
		format!("{}-{}-{}-{}-{}-{}-{}",self.firmware[0],self.firmware[1],self.firmware[2],self.firmware[3],self.firmware[4],self.firmware[5],self.firmware[6])
	}

	/// get a filename version of the firmware version like 20-10-1-5-0-0-9.srec
	fn as_filename(&self) -> String {
		format!("{}.srec",self.as_string())
	}
}

impl Display for FirmwareVersion {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f,"{}", self.as_filename())
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
			Self::Scan => "scan",
			Self::Update => "update",
			Self::Overwrite => "overwrite",
		})
	}
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

impl ControllerTypes {
	fn get_empty_modules_file(&self) -> String {
    	match self {
			Self::ModulineIV => String::from(":::::::
:::::::
:::::::
:::::::"),
			Self::ModulineMini => String::from(":::
:::
:::
:::"),
			Self::ModulineDisplay => String::from(":
:
:
:"),
		}
	}
}

struct Module {
	slot: u8,
	spidev: Spidev,
	interrupt: Lines<Input>,
	firmware: FirmwareVersion,
	manufacturer: u32,
	qr_front: u32,
	qr_back: u32,
}

impl Module {
	/// construct a new module at the given slot for the given controller type
	async fn new(slot:u8, controller: &ControllerTypes) -> Option<Self> {
		//get the spidev
		let (mut spidev, mut interrupt) = match controller { //get the Interrupt GPIO and the spidev
			ControllerTypes::ModulineIV => {
				match slot {
					1 => {
						let chip = Chip::new("gpiochip0").await.unwrap(); //pin 156
						let opts = Options::input([6])
							.edge(EdgeDetect::Falling)
							.consumer("module 1 interrupt");
						let interrupt_pin = chip.request_lines(opts).await.unwrap();
						(Spidev::new(File::open("/dev/spidev1.0").expect(SPI_FILE_FAULT)), interrupt_pin)
					},
					2 => {
						let chip = Chip::new("gpiochip4").await.unwrap(); //pin 150
						let opts = Options::input([20])
							.edge(EdgeDetect::Falling)
							.consumer("module 2 interrupt");
						let interrupt_pin = chip.request_lines(opts).await.unwrap();
						(Spidev::new(File::open("/dev/spidev1.1").expect(SPI_FILE_FAULT)), interrupt_pin)
					},
					3 => {
						let chip = Chip::new("gpiochip0").await.unwrap(); //pin 157
						let opts = Options::input([7])
							.edge(EdgeDetect::Falling)
							.consumer("module 3 interrupt");
						let interrupt_pin = chip.request_lines(opts).await.unwrap();
						(Spidev::new(File::open("/dev/spidev2.0").expect(SPI_FILE_FAULT)), interrupt_pin)
					},
					4 => {
						let chip = Chip::new("gpiochip4").await.unwrap(); //pin 151
						let opts = Options::input([21])
							.edge(EdgeDetect::Falling)
							.consumer("module 4 interrupt");
						let interrupt_pin = chip.request_lines(opts).await.unwrap();
						(Spidev::new(File::open("/dev/spidev2.1").expect(SPI_FILE_FAULT)), interrupt_pin)
					},
					5 => {
						let chip = Chip::new("gpiochip4").await.unwrap(); //pin 91
						let opts = Options::input([1])
							.edge(EdgeDetect::Falling)
							.consumer("module 5 interrupt");
						let interrupt_pin = chip.request_lines(opts).await.unwrap();
						(Spidev::new(File::open("/dev/spidev2.2").expect(SPI_FILE_FAULT)),interrupt_pin)
					},
					6 => {
						let chip = Chip::new("gpiochip3").await.unwrap(); //pin 85
						let opts = Options::input([26])
							.edge(EdgeDetect::Falling)
							.consumer("module 6 interrupt");
						let interrupt_pin = chip.request_lines(opts).await.unwrap();
						(Spidev::new(File::open("/dev/spidev2.3").expect(SPI_FILE_FAULT)), interrupt_pin)
					},
					7 => {
						let chip = Chip::new("gpiochip2").await.unwrap(); //pin 77
						let opts = Options::input([19])
							.edge(EdgeDetect::Falling)
							.consumer("module 7 interrupt");
						let interrupt_pin = chip.request_lines(opts).await.unwrap();
						(Spidev::new(File::open("/dev/spidev0.0").expect(SPI_FILE_FAULT)), interrupt_pin)
					},
					8 => {
						let chip = Chip::new("gpiochip2").await.unwrap(); //pin 74
						let opts = Options::input([22])
							.edge(EdgeDetect::Falling)
							.consumer("module 8 interrupt");
						let interrupt_pin = chip.request_lines(opts).await.unwrap();
						(Spidev::new(File::open("/dev/spidev0.1").expect(SPI_FILE_FAULT)), interrupt_pin)
					},
					_ => {
						eprintln!("For the Moduline IV, slot should be a value from 1-8 but it was {}",slot);
						return None;
					},
				}
			},
			ControllerTypes::ModulineMini => {
				match slot {
					1 => {
						let chip = Chip::new("gpiochip0").await.unwrap(); //pin 161
						let opts = Options::input([10])
							.edge(EdgeDetect::Falling)
							.consumer("module 1 interrupt");
						let interrupt_pin = chip.request_lines(opts).await.unwrap();
						(Spidev::new(File::open("/dev/spidev1.0").expect(SPI_FILE_FAULT)), interrupt_pin)
					},
					2 => {
						let chip = Chip::new("gpiochip0").await.unwrap(); //pin 155
						let opts = Options::input([5])
							.edge(EdgeDetect::Falling)
							.consumer("module 2 interrupt");
						let interrupt_pin = chip.request_lines(opts).await.unwrap();
						(Spidev::new(File::open("/dev/spidev1.1").expect(SPI_FILE_FAULT)), interrupt_pin)
					},
					3 => {
						let chip = Chip::new("gpiochip3").await.unwrap(); //pin 85
						let opts = Options::input([26])
							.edge(EdgeDetect::Falling)
							.consumer("module 3 interrupt");
						let interrupt_pin = chip.request_lines(opts).await.unwrap();
						(Spidev::new(File::open("/dev/spidev2.0").expect(SPI_FILE_FAULT)),interrupt_pin)
					},
					4 => {
						let chip = Chip::new("gpiochip2").await.unwrap(); //pin 77
						let opts = Options::input([19])
							.edge(EdgeDetect::Falling)
							.consumer("module 4 interrupt");
						let interrupt_pin = chip.request_lines(opts).await.unwrap();
						(Spidev::new(File::open("/dev/spidev2.1").expect(SPI_FILE_FAULT)),interrupt_pin)
					},
					_ => {
						eprintln!("For the Moduline Mini, slot should be a value from 1-4 but it was {}",slot);
						return None;
					},
				}
			},
			ControllerTypes::ModulineDisplay => {
				match slot {
					1 => {
						let chip = Chip::new("gpiochip3").await.unwrap(); //pin 43
						let opts = Options::input([5])
							.edge(EdgeDetect::Falling)
							.consumer("module 1 interrupt");
						let interrupt_pin = chip.request_lines(opts).await.unwrap();
						(Spidev::new(File::open("/dev/spidev1.0").expect(SPI_FILE_FAULT)),interrupt_pin)
					},
					2 => {
						let chip = Chip::new("gpiochip0").await.unwrap(); //pin 152
						let opts = Options::input([0])
							.edge(tokio_gpiod::EdgeDetect::Falling)
							.consumer("module 2 interrupt");
						let interrupt_pin = chip.request_lines(opts).await.unwrap();
						(Spidev::new(File::open("/dev/spidev1.1").expect(SPI_FILE_FAULT)), interrupt_pin)
					},
					_ => {
						eprintln!("For the Moduline Display, slot should be a value from 1-2 but it was {}",slot);
						return None;
					},
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
			interrupt,
			firmware: FirmwareVersion { firmware: [0;7] },
			manufacturer: 0,
			qr_front: 0,
			qr_back: 0,
		};
		module.get_module_info()
	}
	
	/// get information from the module like firmware, manufacture, qr codes
	fn get_module_info(mut self) -> Option<Self> {
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
	
		if rx_buf[BOOTMESSAGE_LENGTH-1] != calculate_checksum(&rx_buf, BOOTMESSAGE_LENGTH-1) || (rx_buf[0] != 9 && rx_buf[2] != 9) {
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

	/// Overwrite the firmware on a module \
	/// 
	/// Firmware uploading mechanism \
	/// Because of the parallel spi communication, the feedback from the module is about the previous message that was sent. \
	/// So, after the first message you receive junk, after the second message you receive info if the first message was sent correctly. \
	/// Two ways to fix this: \
	/// The old, send a line of firmware, then send a status request to check if it was uploaded correctly, try again if not, move on to the next line if it was. \
	/// This requires at least two messages sent per line of firmware, theoretically doubling the time to upload one piece of firmware.
	///
	/// The new fast but but complex way, keep track of the line of which you will receive feedback while also keeping track of what you are currently sending, \
	/// this gets complicated once errors start happening. The diagrams below will explain what happens in which situation: \
	/// normal function: \
	/// ``` text
	/// | 0 /\  ||      | 1 /\  ||      | 2 /\  ||      | 3 /\  ||      | 4 /\  ||      | 5 /\  ||      | 6 /\  ||      | 7 /\  ||      | 8 /\  ||      |
	/// |   ||  \/ignore|   ||  \/ 0    |   ||  \/ 1    |   ||  \/ 2    |   ||  \/ 3    |   ||  \/ 4    |   ||  \/ 5    |   ||  \/ 6    |   ||  \/ 7    |
	/// | lineNum    0  | lineNum    1  | lineNum    2  | lineNum    3  | lineNum    4  | lineNum    5  | lineNum    6  | lineNum    7  | lineNum    8  |
	/// | lineCheck MAX | lineCheck  0  | lineCheck  1  | lineCheck  2  | lineCheck  3  | lineCheck  4  | lineCheck  5  | lineCheck  6  | lineCheck  7  |
	/// | errorCount 0  | errorCount 0  | errorCount 0  | errorCount 0  | errorCount 0  | errorCount 0  | errorCount 0  | errorCount 0  | errorCount 0  |
	/// ```
	/// on error swap lineNum and lineCheck, on success after odd number of errors swap them and add one to lineNum \
	/// repeated single/odd number of errors
	/// ``` text
	/// | 0 /\  ||      | 1 /\  ||      | 2 /\  ||      | 3 /\  ||      | 2 /\  ||      | 4 /\  ||      | 2 /\  ||      | 5 /\  ||      | 6 /\  ||      |
	/// |   ||  \/ignore|   ||  \/ 0    |   ||  \/ 1    |   ||  \/ err  |   ||  \/ 3    |   ||  \/ err  |   ||  \/ 4    |   ||  \/ 2    |   ||  \/ 5    |
	/// | lineNum    0  | lineNum    1  | lineNum    2  | lineNum    3  | lineNum    2  | lineNum    4  | lineNum    2  | lineNum    5  | lineNum    6  |
	/// | lineCheck MAX | lineCheck  0  | lineCheck  1  | lineCheck  2  | lineCheck  3  | lineCheck  2  | lineCheck  4  | lineCheck  2  | lineCheck  5  |
	/// | errorCount 0  | errorCount 0  | errorCount 0  | errorCount 1  | errorCount 0  | errorCount 1  | errorCount 0  | errorCount 0  | errorCount 0  |
	/// ```
	/// repeated even number of errors
	/// ``` text
	/// | 0 /\  ||      | 1 /\  ||      | 2 /\  ||      | 3 /\  ||      | 2 /\  ||      | 3 /\  ||      | 4 /\  ||      | 5 /\  ||      | 6 /\  ||      |
	/// |   ||  \/ignore|   ||  \/ 0    |   ||  \/ 1    |   ||  \/ err  |   ||  \/ err  |   ||  \/ 2    |   ||  \/ 3    |   ||  \/ 4    |   ||  \/ 5    |
	/// | lineNum    0  | lineNum    1  | lineNum    2  | lineNum    3  | lineNum    2  | lineNum    3  | lineNum    4  | lineNum    5  | lineNum    6  |
	/// | lineCheck MAX | lineCheck  0  | lineCheck  1  | lineCheck  2  | lineCheck  3  | lineCheck  2  | lineCheck  3  | lineCheck  4  | lineCheck  5  |
	/// | errorCount 0  | errorCount 0  | errorCount 0  | errorCount 1  | errorCount 2  | errorCount 0  | errorCount 0  | errorCount 0  | errorCount 0  |
	/// ```
	/// end of firmware
	/// ``` text
	/// | n-1 /\  ||    | test/\  ||    | n /\  ||      | test/\  ||                    |
	/// |     ||  \/ n-2|     ||  \/ n-1|   ||  \/ n-1  |     ||  \/ firmware response  |
	/// | lineNum    n-1| lineNum    n  | lineNum    n  | lineNum    n                  |
	/// | lineCheck  n-2| lineCheck  n-1| lineCheck  n-1| lineCheck  n                  |
	/// | errorCount 0  | errorCount 0  | errorCount 0  | errorCount 0                  |
	/// ```
	/// end of firmware with error
	/// ``` text
	/// | n-1 /\  ||    | test/\  ||    | n-1 /\  ||    | test/\  ||    | n /\  ||      | test/\  ||    | n /\  ||      | test/\  ||                    |
	/// |     ||  \/ n-2|     ||  \/ err|     ||  \/junk|     ||  \/ n-1|   ||  \/ n-1  |     ||  \/ err|   ||  \/ junk |     ||  \/ firmware response  |
	/// | lineNum    n-1| lineNum    n  | lineNum    n-1| lineNum    n  | lineNum    n  | lineNum    n  | lineNum    n  | lineNum    n                  |
	/// | lineCheck  n-2| lineCheck  n-1| lineCheck  n  | lineCheck  n-1| lineCheck  n-1| lineCheck  n  | lineCheck  n  | lineCheck  n                  |
	/// | errorCount 0  | errorCount 1  | errorCount 2  | errorCount 0  | errorCount 0  | errorCount 0  | errorCount 0  | errorCount 0                  |
	///```
	async fn overwrite_module(&mut self, new_firmware: &FirmwareVersion, multi_progress: MultiProgress, style: ProgressStyle) -> Result<(), UploadError> {
		let mut tx_buf_escape = [0u8;BOOTMESSAGE_LENGTH_CHECK];
		let mut rx_buf_escape = [0u8;BOOTMESSAGE_LENGTH_CHECK];

		let mut tx_buf = [0u8;BOOTMESSAGE_LENGTH+1];
		let mut rx_buf = [0u8;BOOTMESSAGE_LENGTH+1];

		//open and read the firmware file
		let firmware_content_string = match fs::read_to_string(format!("/usr/module-firmware/{}",new_firmware.as_filename())) {
			Ok(file) => file,
			Err(err) => {
				eprintln!("Error: could not read {}\n{}",new_firmware.as_filename(),err);
				return Err(UploadError::FirmwareUntouched(self.slot));
			}
		};

		//upload
		let lines: Vec<&str> = firmware_content_string.split('\n').collect();
		let mut line_number: usize = 0;
		#[allow(unused_assignments)]
		let mut send_buffer_pointer: usize = 0;
		#[allow(unused_assignments)]
		let mut message_pointer: usize = 0;
		let mut message_type: u8 = 0;
		let mut firmware_line_check: usize = usize::MAX; //set line check to usize::MAX for the first message so we know its the first message
		let mut firmware_error_counter: u8 = 0;

		if lines.len() <= 1 {
			eprintln!("Error: firmware file corrupt");
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
			Err(err) => {
				eprintln!("Error: failed spi transfer {}",err);
				return Err(UploadError::FirmwareUntouched(self.slot))
			},
		}

		let spinner = multi_progress.add(ProgressBar::new_spinner());
		spinner.set_message(format!("Wiping old firmware on slot {}",self.slot));
		spinner.enable_steady_tick(Duration::from_millis(100));

		thread::sleep(Duration::from_millis(2500)); // give the module time to wipe the old firmware
		spinner.finish_and_clear();

		let progress = multi_progress.add(ProgressBar::new(lines.len() as u64));
		progress.set_style(style);
		progress.set_message(format!("Uploading firmware {} to slot {}",self.firmware.as_string(), self.slot));


		while message_type != 7 { // 7 marks the last line of the .srec file
			message_type = u8::from_str_radix(lines[line_number].get(1..2).unwrap(), 16).unwrap();

			let line_length = u8::from_str_radix(lines[line_number].get(2..4).unwrap(), 16).unwrap();

			//first time the last line is reached, it is not allowed to send the last line, as it could cause the module to jump to the firmware, potentially leaving line n-1 with an error
			if message_type == 7 && firmware_line_check != line_number {
				//prepare dummy message to get feedback from the previous message
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
						} else {
							firmware_error_counter += 1;
							mem::swap(&mut line_number, &mut firmware_line_check);
							message_type = 0; //last message failed, set the message type to not 7 again so we don't exit the while loop
							thread::sleep(Duration::from_millis(5));
							continue;
						}
					},
					Err(_) => {
						firmware_error_counter += 1;
						mem::swap(&mut line_number, &mut firmware_line_check);
						message_type = 0; //last message failed, set the message type to not 7 again so we don't exit the while loop
						thread::sleep(Duration::from_millis(5));
						continue;
					},
				}
			}
			// prepare firmware message
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

			match self.spidev.transfer(&mut SpidevTransfer::read_write(&tx_buf, &mut rx_buf)) {
				Ok(_) => {
					thread::sleep(Duration::from_millis(1));
					// the first message will always receive junk, ignore this junk and continue to line 1
					if firmware_line_check == usize::MAX {
						line_number +=1;
						firmware_line_check = 0;
						continue;
					}

					if rx_buf[BOOTMESSAGE_LENGTH-1] == calculate_checksum(&rx_buf, BOOTMESSAGE_LENGTH-1) && 
					firmware_line_check == u16::from_be_bytes(clone_into_array(rx_buf.get(6..8).unwrap())) as usize &&
					rx_buf[8] == 1 { // checksum correct?
						if firmware_error_counter & 0b1 > 0{ // if the error counter is uneven swap line number and the line being checked
							std::mem::swap(&mut line_number, &mut firmware_line_check);
						} else { // else set the check number to the line line number, line number will be incremented later if necessary
							firmware_line_check = line_number;
						}
						// the last message needs to be handled differently as it will instantly jump to the firmware when this message is received correctly.
						if message_type == 7 {
							// prepare a dummy message to see if we get a response from the firmware or from the bootloader.
							tx_buf_escape[0] = 49;
							tx_buf_escape[1] = (BOOTMESSAGE_LENGTH-1) as u8;
							tx_buf_escape[2] = 49;
							tx_buf_escape[BOOTMESSAGE_LENGTH-1] = calculate_checksum(&tx_buf_escape, BOOTMESSAGE_LENGTH-1);
							thread::sleep(Duration::from_millis(5));
							_=self.spidev.transfer(&mut SpidevTransfer::read_write(&tx_buf_escape, &mut rx_buf_escape));
							if rx_buf_escape[rx_buf_escape[1] as usize] == calculate_checksum(&rx_buf_escape, rx_buf_escape[1] as usize) && rx_buf_escape[6] == 20 {
								// received response from bootloader, finish the last line of the progress bar and let the while loop exit.
								progress.inc(1);
							} else {
								// last message failed, set the message type to not 7 again so we don't exit the while loop and try again instead
								message_type = 0;
							}
						} else {
							// normal firmware message
							line_number += 1;
							firmware_error_counter = 0;
							progress.inc(1);
						}
					} else {
						mem::swap(&mut line_number, &mut firmware_line_check);
						message_type = 0;
						firmware_error_counter += 1;

						if firmware_error_counter > 10 {
							eprintln!("Error: upload Failed");
							progress.finish_and_clear();
							return Err(UploadError::FirmwareCorrupted(self.slot));
						}
					}
				},
				Err(_) => {
					mem::swap(&mut line_number, &mut firmware_line_check);
					message_type = 0;
					firmware_error_counter += 1;

					if firmware_error_counter > 10 {
						eprintln!("Error: upload Failed, spi transfer failed");
						progress.finish_and_clear();
						return Err(UploadError::FirmwareCorrupted(self.slot));
					}
				}
			}
		}
		progress.finish_and_clear();
		self.cancel_firmware_upload(&mut tx_buf);
		Ok(())
	}

	/// Update a module, checking for new matching firmwares in the firmwares parameter \
	/// The outer Result<Result, UploadError> indicates whether there was an error in the upload process \
	/// The inner Result<Module,Module> indicates whether there was an available update or not.
	async fn update_module(mut self, firmwares: &[FirmwareVersion], multi_progress: MultiProgress, style: ProgressStyle) -> Result<Result<Self, Self>, UploadError> {
		if let Some((index,_junk)) = firmwares.iter().enumerate()
			.filter(|(_i,available)| available.get_hardware() == self.firmware.get_hardware())//filter out incorrect hardware versions
			.filter(|(_i,available)| (available.get_software() > self.firmware.get_software() || self.firmware.get_software() == [255u8,255,255]) && available.get_software() != [255u8,255,255])//filter out wrong software versions
			.map(|(i,available)| (i,available.get_software()))//turn them all into software versions
			.reduce(|acc,(i, software)| if acc.1 < software { (i, software) } else { acc }) //cant use min/max because of the tuple, have to manually compare it in a reduce function
		{
			println!("updating slot {} from {} to {}", self.slot, self.firmware.as_string(), firmwares.get(index).unwrap().as_string());
			match self.overwrite_module(firmwares.get(index).unwrap(),multi_progress, style).await {
				Ok(()) => {
					self.firmware = *firmwares.get(index).unwrap();
					Ok(Ok(self)) //firmware updated successfully
				},
				Err(err) => Err(err), //error uploading the new firmware
			}
		} else { // no new firmware found to update the module with.
			Ok(Err(self))
		}
	}

	/// Cancel the firmware upload of the module bringing the module into operational state
	fn cancel_firmware_upload(&mut self, tx_buf: &mut [u8]) {
		tx_buf[0] = 19;
		tx_buf[1] = (BOOTMESSAGE_LENGTH-1) as u8;
		tx_buf[2] = 19;
		tx_buf[BOOTMESSAGE_LENGTH-1] = calculate_checksum(tx_buf, BOOTMESSAGE_LENGTH-1);
		_=self.spidev.transfer(&mut SpidevTransfer::write(tx_buf));
	}
}

impl Display for Module {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let hardware = self.firmware.get_hardware();
		let software = self.firmware.get_software();
		write!(f, "{}", match hardware[1] {
			10 => match hardware[2] {
				1 => format!("slot {}: 6 Channel Input module version {} sw: {}.{}.{}", self.slot, hardware[3], software[0], software[1], software[2]),
				2 => format!("slot {}: 10 Channel Input module version {} sw: {}.{}.{}", self.slot, hardware[3], software[0], software[1], software[2]),
				3 => format!("slot {}: 4-20mA Input module version {} sw: {}.{}.{}", self.slot, hardware[3], software[0], software[1], software[2]),
				_ => format!("slot {}: unknown: {}",self.slot,self.firmware.as_string()),
			},
			20 => match hardware[2] {
				1 => format!("slot {}: 2 Channel Output module version {} sw: {}.{}.{}", self.slot, hardware[3], software[0], software[1], software[2]),
				2 => format!("slot {}: 6 Channel Output module version {} sw: {}.{}.{}", self.slot, hardware[3], software[0], software[1], software[2]),
				3 => format!("slot {}: 10 Channel Output module version {} sw: {}.{}.{}", self.slot, hardware[3], software[0], software[1], software[2]),
				_ => format!("slot {}: unknown: {}", self.slot, self.firmware.as_string()),
			},
			30 => match hardware[2] {
				3 => format!("slot {}: ANLEG IR module version {} sw: {}.{}.{}", self.slot, hardware[3], software[0], software[1], software[2]),
				_ => format!("slot {}: unknown: {}", self.slot, self.firmware.as_string()),
			},
			40 => match hardware[2] {
				1 => format!("slot {}: ANLEG RTC Control module version {} sw: {}.{}.{}", self.slot, hardware[3], software[0], software[1], software[2]),
				_ => format!("slot {}: unknown: {}", self.slot, self.firmware.as_string()),
			}
			_ => format!("slot {}: unknown: {}", self.slot, self.firmware.as_string()),
		})
	}
}

/// error out and restart nodered and go-simulink if required
fn err_n_restart_services(nodered: bool, simulink: bool) -> ! {
	if nodered {
		_ = Command::new("systemctl")
			.arg("start")
			.arg("nodered")
			.status();
	}

	if simulink {
		_ = Command::new("systemctl")
			.arg("start")
			.arg("go-simulink")
			.status();
	}
	exit(-1);
}

/// exit with a success code and restart the nodered and go-simulink services if required
fn success(nodered: bool, simulink: bool) -> ! {
	if nodered {
		_ = Command::new("systemctl")
			.arg("start")
			.arg("nodered")
			.status();
	}

	if simulink {
		_ = Command::new("systemctl")
			.arg("start")
			.arg("go-simulink")
			.status();
	}
	exit(0);
}

/// error out without restarting any services
fn err_n_die(message: &str) -> ! {
	eprintln!("{}",message);
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
async fn get_modules(controller: &ControllerTypes) -> Vec<Module> {
	let mut modules = Vec::with_capacity(8);
	let mut set = tokio::task::JoinSet::new();
	let controller = *controller;
	for i in 1..controller as usize {
		set.spawn(async move {
			Module::new(i as u8, &controller).await
		});
	}
	for _ in 1..controller as usize {
		if let Some(Ok(Some(module))) = set.join_next().await {
			modules.push(module);
		}
	}
	modules

}

/// get the modules in the controller and save them
async fn get_modules_and_save(controller: &ControllerTypes) -> Vec<Module> {
	let modules = get_modules(controller).await;
	let mut modules_out: Vec<Option<Module>> = match &controller {
		ControllerTypes::ModulineDisplay => vec![None,None],
		ControllerTypes::ModulineIV => vec![None,None,None,None,None,None,None,None],
		ControllerTypes::ModulineMini => vec![None,None,None,None],
	};
	for module in modules {
		let slot = module.slot;
		modules_out[(slot -1) as usize] = Some(module);
	}
	save_modules(modules_out,controller)
}

/// save all the modules to modules to /usr/module-firmware/modules.txt, None elements will be removed from the file
fn save_modules(modules: Vec<Option<Module>>, controller: &ControllerTypes) -> Vec<Module> {
	let modules_string = if let Ok(contents) = std::fs::read_to_string("/usr/module-firmware/modules.txt") {
		if contents.split('\n').count() == 4 { // for some reason the file from older systems is messed up sometimes
			contents
		} else {
			controller.get_empty_modules_file()
		}
	} else { //if the file doesn't exist, generate a new template
		controller.get_empty_modules_file()
	};
	let mut lines: Vec<String> = modules_string.split('\n').map(|element| element.to_owned()).collect();
	let mut firmwares: Vec<String> = lines.get_mut(0).unwrap().split(':').map(|element| element.to_owned()).collect();
	let mut manufactures: Vec<String> = lines.get_mut(1).unwrap().split(':').map(|element| element.to_owned()).collect();
	let mut front_qrs: Vec<String> = lines.get_mut(2).unwrap().split(':').map(|element| element.to_owned()).collect();
	let mut rear_qrs: Vec<String> = lines.get_mut(3).unwrap().split(':').map(|element| element.to_owned()).collect();

	for (i,module) in modules.iter().enumerate() {
		if let Some(module) = module {
			*firmwares.get_mut((module.slot-1) as usize).unwrap() = module.firmware.as_string();
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

async fn update_one_module(module: Module, available_firmwares: &[FirmwareVersion], multi_progress: MultiProgress, style: ProgressStyle, controller: ControllerTypes, nodered: bool, simulink: bool) -> ! {
    match module.update_module(available_firmwares, multi_progress, style).await {
		Ok(Ok(module)) => {
			println!("Succesfully updated slot {} to {}", module.slot,module.firmware.as_string());
			save_modules(vec![Some(module)], &controller);
			success(nodered, simulink);
		},
		Err(err) => match err {
			UploadError::FirmwareCorrupted(slot) => {
				err_n_die(format!("Update failed, firmware is corrupted on slot {}", slot).as_str());
			},
			UploadError::FirmwareUntouched(slot) => {
				eprintln!("Update failed on slot {}", slot);
				err_n_restart_services(nodered, simulink);
			}
		},
		Ok(Err(module)) => {
			eprintln!("Update failed, no update available for slot {}: {}", module.slot, module.firmware.as_string());
			err_n_restart_services(nodered, simulink);
		}
	}
}

async fn update_all_modules(modules: Vec<Module>, available_firmwares: &[FirmwareVersion], multi_progress: &MultiProgress, style: &ProgressStyle, controller: ControllerTypes, nodered: bool, simulink: bool) -> ! {
    let mut upload_results = Vec::with_capacity(modules.len());
    let mut new_modules = Vec::with_capacity(modules.len());
    let mut firmware_corrupted = false;
    let mut set = tokio::task::JoinSet::new();
	for module in modules {
		let available_firmwares = available_firmwares.to_owned();
		let multi_progress = multi_progress.clone();
		let style = style.clone();
		set.spawn(async move {
			module.update_module(available_firmwares.as_slice(), multi_progress, style).await
		});
	}
	for _ in 0..new_modules.len() {
		upload_results.push(set.join_next().await.unwrap().unwrap());
	}
    for result in upload_results {
		match result {
			Ok(Ok(module)) => { //module updated
				new_modules.push(Some(module))
			},
			Err(err) => match err {
				UploadError::FirmwareCorrupted(slot) => {
					eprintln!("Update failed, firmware is corrupted on slot {}",slot);
					firmware_corrupted = true;
				},
				UploadError::FirmwareUntouched(slot) => {
					eprintln!("Update failed on slot {}", slot);
				}
			},
			Ok(Err(_)) => (), //no new firmwares available
		}
	}
    if !new_modules.is_empty() {
		println!("Succesfully updated:");
		for module in &new_modules {
			println!("slot {} to {}", module.as_ref().unwrap().slot, module.as_ref().unwrap().firmware.as_string());
		}
	} else if !firmware_corrupted {
		eprintln!("No updates found for the modules in this controller.");
	}
    save_modules(new_modules, &controller);
    if firmware_corrupted {
		err_n_die("could not restart nodered and go-simulink services due to corrupted firmware.");	
	}
					
    success(nodered, simulink);
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
	//get the controller hardware
	let hardware_string= fs::read_to_string("/sys/firmware/devicetree/base/hardware").unwrap_or_else(|_|{
		err_n_die("Could not find a hardware description file, this feature is not supported by your hardware.");
	});

	let controller = if hardware_string.contains("Moduline IV") {
		ControllerTypes::ModulineIV
	} else if hardware_string.contains("Moduline Mini") {
		ControllerTypes::ModulineMini
	} else if hardware_string.contains("Moduline Screen") {
		ControllerTypes::ModulineDisplay
	} else {
		err_n_die(format!("{} does not exist. Can't proceed",hardware_string).as_str());
	};

	//stop services potentially trying to use the module
	let output = Command::new("systemctl")
		.arg("is-active")
		.arg("nodered")
		.output().unwrap().stdout;

	let nodered = !String::from_utf8_lossy(&output).into_owned().contains("in");

	let output =Command::new("systemctl")
		.arg("is-active")
		.arg("go-simulink")
		.output().unwrap().stdout;

	let simulink = !String::from_utf8_lossy(&output).into_owned().contains("in");

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

	match ctrlc::set_handler(move || {
		err_n_restart_services(nodered, simulink)
	}) {
		Ok(()) => (),
		Err(err) => {
			eprintln!("couldn't set sigint handler: {}", err);
			err_n_restart_services(nodered, simulink);
		}
	}

	//start getting module information in a seperate thread while other init is happening
	let modules_fut = get_modules_and_save(&controller);

	//get all the firmwares
	let available_firmwares: Vec<FirmwareVersion> = fs::read_dir("/usr/module-firmware/").unwrap_or_else(|_| {
		err_n_restart_services(nodered, simulink);
	}) // get the files in module-firmware
	.map(|file| file.unwrap().file_name().to_str().unwrap().to_string()) //turn them into strings
	.filter(|file_name| file_name.ends_with(".srec")) //keep only the srec files
	.map(|firmware| FirmwareVersion::from_filename(firmware).unwrap())//turn them into FirmwareVersion Structs
	.collect(); //collect them into a vector

	//create the base for the progress bar(s)
	let multi_progress = MultiProgress::new();
	let style = ProgressStyle::with_template(
		"{bar:40.cyan/blue} {pos:>7}/{len:7} ({eta})",
	)
	.unwrap()
	.progress_chars("##-")
	.with_key("eta", |state: &ProgressState, w: &mut dyn Write| write!(w, "{:.1}s",state.eta().as_secs_f64()).unwrap());

	let command = if let Some(arg) = env::args().nth(1) {
		match arg.as_str() {
			"scan" => CommandArg::Scan,
			"update" => CommandArg::Update,
			"overwrite" => CommandArg::Overwrite,
			_ => {
				eprintln!("Invalid command entered {}\n{}",arg, USAGE);
				err_n_restart_services(nodered, simulink);
			}
		}
	} else {
		Select::new("What do you want to do?", vec![CommandArg::Scan, CommandArg::Update, CommandArg::Overwrite]).prompt().unwrap_or_else(|_| err_n_restart_services(nodered, simulink))
	};

	//get the modules from the previously started thread
	let modules =  modules_fut.await;

	match command {
		CommandArg::Scan => {
			//scan and save has already been done before this option was even selected, print out the values and exit
			if !modules.is_empty() {
				println!("Found modules:");
				for module in &modules {
					println!("{}", module);
				}
			} else {
				println!("No modules found")
			}
			success(nodered, simulink);
		},


		CommandArg::Update => {
			//find the update type
			if let Some(arg) = env::args().nth(2) {
				match arg.as_str() {
					"all" => update_all_modules(modules, &available_firmwares, &multi_progress, &style, controller, nodered, simulink).await,
					_ => if let Ok(slot) = arg.parse::<u8>() {
						if slot < controller as u8 || slot >= 1 {
							let module = Module::new(slot, &controller).await.unwrap_or_else(||{
								eprintln!("Couldn't find a module in slot {}", slot);
								err_n_restart_services(nodered, simulink);
							});
							update_one_module(module, &available_firmwares, multi_progress, style, controller, nodered, simulink).await;
						} else {
							eprintln!("{}", USAGE);
							err_n_restart_services(nodered, simulink);
						}
					} else {
						eprintln!("{}", USAGE);
						err_n_restart_services(nodered, simulink);
					}
				}
			} else {
				match Select::new("Update one module or all?", vec!["all", "one"]).prompt().unwrap_or_else(|_| err_n_restart_services(nodered, simulink)) {
					"all" =>  update_all_modules(modules, &available_firmwares, &multi_progress, &style, controller, nodered, simulink).await,
					"one" => {
						if !modules.is_empty() {
							match Select::new("select a module to update", modules).with_page_size(8).prompt() {
								Ok(module) => update_one_module(module, &available_firmwares, multi_progress, style, controller, nodered, simulink).await,
								Err(_) => {
									err_n_restart_services(nodered, simulink);
								}
							}
						} else {
							eprintln!("No modules found in the controller.");
							err_n_restart_services(nodered, simulink);
						}
					}
					_ => {
						eprintln!("You shouldn't be here, turn back to whence you came");
						err_n_restart_services(nodered, simulink);
					}
				}
			};
		},


		CommandArg::Overwrite => {
			let mut module = if let Some(arg) = env::args().nth(2) {
				if let Ok(slot) = arg.parse::<u8>() {
					if let Some(module) = Module::new(slot, &controller).await {
						module
					} else {
						eprintln!("No module present in that slot");
						err_n_restart_services(nodered, simulink);
					}
				} else {
					eprintln!("Invalid slot entered\n{}", USAGE);
					err_n_restart_services(nodered, simulink);
				}
			} else if !modules.is_empty() {
				Select::new(SLOT_PROMPT, modules).with_page_size(8).prompt().unwrap_or_else(|_| err_n_restart_services(nodered, simulink))
			} else {
				eprintln!("No modules found in the controller.");
				err_n_restart_services(nodered, simulink);
			};

			let new_firmware = if let Some(arg) = env::args().nth(3) {
				if let Some(firmware) = FirmwareVersion::from_filename(arg.clone()) {
					if available_firmwares.contains(&firmware){
						firmware
					} else {
						eprintln!("/usr/module-firmware/{} does not exist",arg);
						err_n_restart_services(nodered, simulink);
					}
				} else {
					eprintln!("Invalid firmware entered\n{}", USAGE);
					err_n_restart_services(nodered, simulink);
				}
			} else {
				let valid_firmwares: Vec<&FirmwareVersion> = available_firmwares.iter()
					.filter(|firmware| firmware.get_hardware() == module.firmware.get_hardware())
					.collect();
				if !valid_firmwares.is_empty() {
					*Select::new("Which firmware to upload?", valid_firmwares).prompt().unwrap_or_else(|_| err_n_restart_services(nodered, simulink))
				} else {
					eprintln!("No firmware(s) found for this module.");
					err_n_restart_services(nodered, simulink);
				}
			};
			match module.overwrite_module(&new_firmware, multi_progress, style).await {
				Ok(()) => {
					println!("succesfully updated slot {} from {} to {}", module.slot, module.firmware.as_string(), new_firmware.as_string());
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
							eprintln!("Update failed on slot {}", slot);
							err_n_restart_services(nodered, simulink);
						}
					}	
				}
			}
		}
		
	}
}