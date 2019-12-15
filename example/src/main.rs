struct MyDevice;

//use strum::EnumMessage;
use scpi::command::Command;
use scpi::{Context, Device};
use scpi::commands::IdnCommand;
use scpi::tree::Node;
use scpi::tokenizer::Tokenizer;
use scpi::error::Error;
use core::fmt;



struct RstCommand;
impl Command for RstCommand {
    fn event(&self, context: &mut Context, _args: &mut Tokenizer) -> Result<(), Error> {
        writeln!(context.writer, "*RST").unwrap();
        context.device.rst()
    }

    fn query(&self, _context: &mut Context, _args: &mut Tokenizer) -> Result<(), Error> {
        Err(Error::UndefinedHeader)
    }
}

struct SensVoltDcCommand;
impl Command for SensVoltDcCommand {
    fn event(&self, _context: &mut Context, args: &mut Tokenizer) -> Result<(), Error> {
        args.next_data(true)?;
        Ok(())
    }

    fn query(&self, context: &mut Context, _args: &mut Tokenizer) -> Result<(), Error> {
        writeln!(context.writer, "SENSe:VOLTage:DC?").unwrap();
        Ok(())
    }
}

struct SensVoltAcCommand;
impl Command for SensVoltAcCommand {
    fn event(&self, _context: &mut Context, _args: &mut Tokenizer) -> Result<(), Error> {
        Err(Error::UndefinedHeader)
    }

    fn query(&self, context: &mut Context, _args: &mut Tokenizer) -> Result<(), Error> {
        writeln!(context.writer, "SENSe:VOLTage:AC?").unwrap();
        Ok(())
    }
}

impl Device for MyDevice {
    fn cls(&mut self) -> Result<(), Error> {
        unimplemented!()
    }

    fn rst(&mut self) -> Result<(), Error> {
        println!("Device reset");
        Ok(())
    }

    fn error_enqueue(&self, _err: Error) -> Result<(), Error> {
        Ok(())
    }

    fn error_dequeue(&self) -> Error {
        Error::NoError
    }

    fn error_len(&self) -> u32 {
        0
    }

    fn error_clear(&self) {

    }

    fn oper_status(&self) -> u16 {
        unimplemented!()
    }

    fn ques_status(&self) -> u16 {
        unimplemented!()
    }

}

struct MyWriter {
    
}

impl fmt::Write for MyWriter {
    fn write_str(&mut self, s: &str) -> Result<(), fmt::Error> {
       print!("{}", s);
        Ok(())
    }
}

macro_rules! node {
    ($name:literal, $handler:expr) => {
        Node {name: $name, handler: Some($handler), sub: None}
    };
    ($name:literal => {$($contents:tt)*} ) => {
        Node {name: $name, handler: None, sub: Some(node!([$($contents)*]))}
    };
    ($name:literal, $handler:expr => {$($contents:tt)*} ) => {
        Node {name: $name, handler: Some($handler), sub: Some(node!([$($contents)*]))}
    };
    ([$($contents:tt)*]) => {&[$($contents)*]};
}

macro_rules! root {
    ($($contents:tt)*) => {
        Node {name: b"ROOT", handler: None, sub: Some(node!([$($contents)*]))}
    };
}

macro_rules! idn {
    ($manufacturer:expr, $model:expr) => {
        &IdnCommand{
            manufacturer: $manufacturer,
            model: $model,
            serial: b"0",
            firmware: b"0"
        }
    };
    ($manufacturer:expr, $model:expr, $serial:expr) => {
        &IdnCommand{
            manufacturer: $manufacturer,
            model: $model,
            serial: $serial,
            firmware: b"0"
        }
    };
    ($manufacturer:expr, $model:expr, $serial:expr, $firmware:expr) => {
        &IdnCommand{
            manufacturer: $manufacturer,
            model: $model,
            serial: $serial,
            firmware: $firmware
        }
    };
}


fn main(){

    let command = b"SENS:VOLT:DC?; *IDN? ; *RST; AC?; :SENSe:VOLTage:DC?";

    let mut my_device = MyDevice { };

    let mut tree = root![
        node!(b"*IDN", idn!(b"GPA-Robotics", b"SCPI-RS")),
        node!(b"*RST", &RstCommand{}),
        node!(b"*CLS", &RstCommand{}),
        node!(b"SENSe" => {
            Node {name: b"VOLTage",
                handler: None,
                sub: Some(&[
                    Node {name: b"DC",
                        handler: Some(&SensVoltDcCommand{}),
                        sub: None
                    },
                    Node {name: b"AC",
                        handler: Some(&SensVoltAcCommand{}),
                        sub: None
                    }
                ])
            },
            Node {name: b"CURRent",
                handler: None,
                sub: Some(&[
                    Node {name: b"DC",
                        handler: None,
                        sub: None
                    },
                    Node {name: b"AC",
                        handler: None,
                        sub: None
                    }
                ])
            }
        })
    ];

    let mut writer = MyWriter{};

    let mut context = Context::new(&mut my_device, &mut writer, &mut tree);

    let mut tokenizer = Tokenizer::from_str(command);

    let result = context.exec(&mut tokenizer);

    if let Err(err) = result {
        println!("Command result: {}", String::from_utf8(err.get_message().unwrap().to_vec()).unwrap());
    }else{
        println!("Command result: Success");
    }



}