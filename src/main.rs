use std::{
	fs::File,
	io::Read,
	env,
	process::{Command, exit},
	time::Duration,
};

use spidev::{Spidev, SpidevOptions, SpiModeFlags, SpidevTransfer};

#[derive(Debug, PartialEq, Eq)]
struct FirmwareVersion {
	firmware: [u8;7],
}

impl FirmwareVersion {
	fn from_filename(name: String) -> Option<Self> {
		let mut firware: [u8;7] = [0u8;7];
		if let Some(no_extension) = name.split(".").next() {
			let numbers = no_extension.split("-");

			for (i, num) in numbers.enumerate() {
				let part = firware.get_mut(i)?;
				if let Ok(file_part) = u8::from_str_radix(num, 10) {
					*part = file_part;
				} else {
					return None;
				}
			}
		}
		Some(Self {
			firmware: firware,
		})
	}

	fn get_software(&self) -> &[u8] {
		self.firmware.get(4..7).unwrap()
	}

	fn get_hardware(&self) -> &[u8] {
		self.firmware.get(0..4).unwrap()
	}
}

const DUMMY_MESSAGE: [u8;5] = [0;5];

const BOOTMESSAGE_LENGTH: usize = 46;
const BOOTMESSAGE_LENGTH_CHECK: usize = 61;

const SPI_FILE_FAULT: &str = "spidev does not exist";

const USAGE: &str = 
"Usage:
flash-module <slot> <file> [force]
<>: must be present
[]: optional

examples:
flash-module 1 20-20-2-6-1-3-2.srec 1 //forced flash of outputmodule firmware to the module in slot 1
flash-module 8 20-10-1-5-2-0-0.srec   //checked flash of inputmodule firmware to the module in slot 8";

fn err_n_restart_services(nodered: bool, simulink: bool) {
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
	}
	exit(-1);
}

fn success(nodered: bool, simulink: bool) {
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
	}
	exit(0);
}

fn err_n_die(message: &str) {
	println!("{}",message);
	exit(-1);
}

fn calculate_checksum(message: &[u8], length: usize) -> u8 {
	let mut checksum: u8 = 0;
	for val in message.get(0..length).unwrap(){
		checksum=checksum.wrapping_add(*val);
	}
	checksum
}

fn reset_module(slot: &u8, state: bool) {
	if state {
		_=std::fs::write(format!("/sys/class/leds/ResetM-{}/brightness",slot), "255");
	} else {
		_=std::fs::write(format!("/sys/class/leds/ResetM-{}/brightness",slot), "0");
	}
}

fn cancel_firmware_upload(spidev: &mut Spidev, tx_buf: &mut [u8]) {
	tx_buf[0] = 19;
	tx_buf[1] = (BOOTMESSAGE_LENGTH-1) as u8;
	tx_buf[2] = 19;
	tx_buf[BOOTMESSAGE_LENGTH-1] = calculate_checksum(&tx_buf, BOOTMESSAGE_LENGTH-1);
	_=spidev.transfer(&mut SpidevTransfer::write(&tx_buf));
}

fn clone_into_array<A, T>(slice: &[T]) -> A
where
	A: Default + AsMut<[T]>,
	T: Clone,
{
	let mut a = A::default();
	<A as AsMut<[T]>>::as_mut(&mut a).clone_from_slice(slice);
	a
}

fn main() {
	let slot = u8::from_str_radix(env::args().nth(1).expect(USAGE).as_str(),10).expect(USAGE);
	let firmware_name = env::args().nth(2).expect(USAGE);
	let forced = match env::args().nth(3) {
		Some(_) => {
			println!("Forcing update, I hope you know what you are doing.");
			true
		},
		None => false,
	};
	let mut firmware_file = File::open(format!("/usr/module-firmware/{}",firmware_name)).expect(format!("Invalid file entered, {} does not exist in /usr/module-firmware/", firmware_name).as_str());

	let new_firmware_version = FirmwareVersion::from_filename(firmware_name).expect("Invalid firmware name");

    let mut hardware_file = File::open("/sys/firmware/devicetree/base/hardware").expect("Your controller does not support this feature");
	let mut hardware_string = String::new();
	_ = hardware_file.read_to_string(&mut hardware_string).unwrap();

	//get the spidev
	let mut spidev = if hardware_string.contains("Moduline IV") { //get the Interrupt GPIO and the spidev
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
	} else if hardware_string. contains("Moduline Mini") {
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
	} else if hardware_string.contains("Moduline Screen") {
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
	} else {
		panic!("Unsupported hardware");
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
			.spawn();
	}

	if simulink {
		_ = Command::new("systemctl")
			.arg("stop")
			.arg("go-simulink")
	}

	spidev.configure(
		&SpidevOptions::new()
		.bits_per_word(8)
		.max_speed_hz(2_000_000)
		.mode(SpiModeFlags::SPI_MODE_0)
		.build()
	).unwrap();

	match spidev.transfer(&mut SpidevTransfer::write(&DUMMY_MESSAGE)) {
		Ok(())=> (),
		Err(_) => err_n_restart_services(nodered, simulink),
	};

	reset_module(&slot, true);

	//give module time to reset
	std::thread::sleep(Duration::from_millis(100));

	reset_module(&slot, false);

	std::thread::sleep(Duration::from_millis(100));

	//check module firmware

	let mut tx_buf = [0u8;BOOTMESSAGE_LENGTH+1];
	let mut rx_buf = [0u8;BOOTMESSAGE_LENGTH+1];

	let mut tx_buf_escape = [0u8;BOOTMESSAGE_LENGTH_CHECK];
	let mut rx_buf_escape = [0u8;BOOTMESSAGE_LENGTH_CHECK];

	tx_buf[0] = 9;
	tx_buf[1] = (BOOTMESSAGE_LENGTH-1) as u8;
	tx_buf[2] = 9;
	tx_buf[BOOTMESSAGE_LENGTH-1] = calculate_checksum(&tx_buf, BOOTMESSAGE_LENGTH-1);

	match spidev.transfer(&mut SpidevTransfer::read_write(&tx_buf, &mut rx_buf)) {
		Ok(()) => (),
		Err(_) => err_n_restart_services(nodered, simulink),		
	}

	if rx_buf[BOOTMESSAGE_LENGTH-1] != calculate_checksum(&rx_buf, BOOTMESSAGE_LENGTH-1) {
		println!("error: Checksum from bootloader not correct {} vs {}", rx_buf[BOOTMESSAGE_LENGTH-1], calculate_checksum(&rx_buf, BOOTMESSAGE_LENGTH-1));
		cancel_firmware_upload(&mut spidev, &mut tx_buf);
		err_n_restart_services(nodered, simulink);
	} else if rx_buf[0] != 9 && rx_buf[2] != 9 {
		println!("error: Wrong response from bootloader");
		cancel_firmware_upload(&mut spidev, &mut tx_buf);
		err_n_restart_services(nodered, simulink);
	}

	let old_firmware = FirmwareVersion {
		firmware: clone_into_array(rx_buf.get(6..13).unwrap()),
	};

	if (new_firmware_version.get_hardware() == old_firmware.get_hardware() && new_firmware_version.get_software() != old_firmware.get_software()) || forced {
		//upload
		tx_buf[0] = 29;
		tx_buf[1] = (BOOTMESSAGE_LENGTH-1) as u8;
		tx_buf[2] = 29;
		let sw = new_firmware_version.get_software();
		tx_buf[6] = sw[0];
		tx_buf[7] = sw[1];
		tx_buf[8] = sw[2];

		tx_buf[BOOTMESSAGE_LENGTH-1] = calculate_checksum(&tx_buf, BOOTMESSAGE_LENGTH-1);

		match spidev.transfer(&mut SpidevTransfer::write(&tx_buf)) {
			Ok(()) => (),
			Err(_) => err_n_restart_services(nodered, simulink),
		}

		std::thread::sleep(Duration::from_millis(2500));

		let mut firmware_content_string = String::with_capacity(firmware_file.metadata().unwrap().len() as usize + 10);
		match firmware_file.read_to_string(&mut firmware_content_string) {
			Ok(_) => (),
			Err(_) => err_n_restart_services(nodered, simulink),
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
			err_n_restart_services(nodered, simulink);
		}

		while message_type != 7 {
			message_type = u8::from_str_radix(lines[line_number].get(1..2).unwrap(), 16).unwrap();

			let line_length = u8::from_str_radix(lines[line_number].get(2..4).unwrap(), 16).unwrap();
			if message_type == 7 && firmware_line_check != line_number {
				tx_buf[0] = 49;
				tx_buf[1] = (BOOTMESSAGE_LENGTH-1) as u8;
				tx_buf[2] = 49;
				tx_buf[BOOTMESSAGE_LENGTH-1] = calculate_checksum(&tx_buf, BOOTMESSAGE_LENGTH-1);
				match spidev.transfer(&mut SpidevTransfer::read_write(&tx_buf, &mut rx_buf)) {
					Ok(()) => {
						if rx_buf[BOOTMESSAGE_LENGTH-1] == calculate_checksum(&rx_buf, BOOTMESSAGE_LENGTH-1) &&
						firmware_line_check == u16::from_be_bytes(clone_into_array(rx_buf.get(6..8).unwrap())) as usize &&
						rx_buf[8] == 1 {
							std::thread::sleep(Duration::from_millis(5));
						}
					},
					Err(_) => {
						firmware_error_counter += 1;
						std::mem::swap(&mut line_number, &mut firmware_line_check);
						message_type = 0;
						std::thread::sleep(Duration::from_millis(5));
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

			_=spidev.transfer(&mut SpidevTransfer::read_write(&tx_buf, &mut rx_buf));
			std::thread::sleep(Duration::from_millis(1));

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
							std::thread::sleep(Duration::from_millis(5));
							_=spidev.transfer(&mut SpidevTransfer::read_write(&tx_buf_escape, &mut rx_buf_escape));
							if !(rx_buf_escape[rx_buf_escape[1] as usize] == calculate_checksum(&rx_buf_escape, rx_buf_escape[1] as usize) && rx_buf_escape[6] == 20) {
								message_type = 0;
							}
						} else {
							line_number += 1;
							firmware_error_counter = 0;
						}
					} else {
						std::mem::swap(&mut line_number, &mut firmware_line_check);
						message_type = 0;
						firmware_error_counter += 1;

						if firmware_error_counter > 10 {
							err_n_die("error: checksum repeatedly didn't match during firmware upload (from module)");
						}
					}
				} else {
					std::mem::swap(&mut line_number, &mut firmware_line_check);
					message_type = 0;
					firmware_error_counter += 1;

					if firmware_error_counter > 10 {
						err_n_die("error: lines didn't match during firmware upload");
					}
				}
			} else {
				std::mem::swap(&mut line_number, &mut firmware_line_check);
				message_type = 0;
				firmware_error_counter += 1;

				if firmware_error_counter > 10 {
					err_n_die("error: checksum repeatedly didn't match during firmware upload (local)");
				}
			}
		}

		println!("firmware update successfull");
		cancel_firmware_upload(&mut spidev, &mut tx_buf);
		success(nodered, simulink);


	} else {
		println!("error: Invalid update detected");
		cancel_firmware_upload(&mut spidev, &mut tx_buf);
		err_n_restart_services(nodered, simulink);
	}
}