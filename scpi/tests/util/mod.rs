use std::collections::VecDeque;

use arrayvec::ArrayVec;
use scpi::{
    error::Result,
    prelude::*,
    scpi1999::{
        status::{GetEventRegister, Operation, Questionable},
        EventRegister, ScpiDevice,
    },
};
use serde::Deserialize;

// #[macro_export]
// macro_rules! check_esr {
//     ($context:ident == $esr:literal) => {
//     execute_str!($context, b"*esr?" => result, response {
//         assert_eq!(result, Ok(()));
//         assert_eq!(response, $esr);
//     });
//     };
//     ($context:ident) => {
//     check_esr!($context == b"0\n");
//     };
// }

pub(crate) struct TestDevice {
    /// Event Status Register
    pub esr: u8,
    /// Event Status Enable register
    pub ese: u8,
    /// Service Request Enable register
    pub sre: u8,
    /// OPERation:ENABle register
    pub operation: EventRegister,
    /// QUEStionable:ENABle register
    pub questionable: EventRegister,
    /// Error queue
    pub errors: VecDeque<Error>,
}

impl TestDevice {
    pub(crate) fn new() -> Self {
        TestDevice {
            esr: 0,
            ese: 0,
            sre: 0,
            operation: Default::default(),
            questionable: Default::default(),
            errors: Default::default(),
        }
    }
}

impl ScpiDevice for TestDevice {
    fn sre(&self) -> u8 {
        self.sre
    }

    fn set_sre(&mut self, value: u8) {
        self.sre = value;
    }

    fn esr(&self) -> u8 {
        self.esr
    }

    fn set_esr(&mut self, value: u8) {
        self.esr = value;
    }

    fn ese(&self) -> u8 {
        self.ese
    }

    fn set_ese(&mut self, value: u8) {
        self.ese = value;
    }
}

impl ErrorQueue for TestDevice {
    fn push_back_error(&mut self, err: Error) {
        self.errors.push_back(err);
    }

    fn pop_front_error(&mut self) -> Error {
        self.errors.pop_front().unwrap_or_default()
    }

    fn num_errors(&self) -> usize {
        self.errors.len()
    }

    fn clear_errors(&mut self) {
        self.errors.clear()
    }
}

impl GetEventRegister<Questionable> for TestDevice {
    fn register(&self) -> &EventRegister {
        &self.questionable
    }

    fn register_mut(&mut self) -> &mut EventRegister {
        &mut self.questionable
    }
}

impl GetEventRegister<Operation> for TestDevice {
    fn register(&self) -> &EventRegister {
        &self.operation
    }

    fn register_mut(&mut self) -> &mut EventRegister {
        &mut self.operation
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Record {
    command: String,
    error: i16,
    response: String,
}

#[allow(dead_code)]
pub fn test_file<D: Device>(dev: &mut D, tree: &Node<D>, path: &str) {
    let mut rdr = csv::ReaderBuilder::default()
        .has_headers(true)
        .comment(Some(b'#'))
        .from_path(path)
        .unwrap();

    for result in rdr.deserialize() {
        // Get test case
        let mut record: Record = result.unwrap();
        record.response = record.response.replace("\\n", "\n");

        // Execute command
        let res = test_execute_str(tree, record.command.as_bytes(), dev);

        // Compare result
        match res {
            Ok(buf) => {
                assert_eq!(record.error, 0, "Expected command error {}", record.command);
                assert!(
                    record
                        .response
                        .as_bytes()
                        .eq_ignore_ascii_case(buf.as_slice()),
                    "Unexpected response {}\n\tExpected: {:?}\n\t  Actual: {:?}",
                    record.command,
                    record.response,
                    std::str::from_utf8(buf.as_slice()).unwrap_or("<Invalid utf8>")
                );
            }
            Err(err) => {
                assert_eq!(
                    record.error,
                    err.get_code(),
                    "Unexpected command error {:?}, {}",
                    err,
                    record.command
                );
            }
        }
        println!("OK {:?}", record);
    }
}

pub fn test_execute_str<D: Device>(
    tree: &Node<D>,
    s: &[u8],
    dev: &mut D,
) -> Result<ArrayVec<u8, 256>> {
    let mut context = Context::new();
    let mut buf = ArrayVec::<u8, 256>::new();
    //Result
    tree.run(s, dev, &mut context, &mut buf)?;
    Ok(buf)
}
