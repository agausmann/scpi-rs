//! Contains basic implementations of mandated IEEE 488.2 commands.
//!
//! Mandatory IEEE488.2 commands:
//!
//! | Mnemonic | Name                                 | 488.2 Section |
//! |----------|--------------------------------------|---------------|
//! | *CLS     | Clear Status Command                 | 10.3          |
//! | *ESE     | Standard Event Status Enable Command | 10.10         |
//! | *ESE?    | Standard Event Status Enable Query   | 10.11         |
//! | *ESR?    | Standard Event Status Register Query | 10.12         |
//! | *IDN?    | Identification Query                 | 10.14         |
//! | *OPC     | Operation Complete Command           | 10.18         |
//! | *OPC?    | Operation Complete Query             | 10.19         |
//! | *RST     | Reset Command                        | 10.32         |
//! | *SRE     | Service Request Enable Command       | 10.34         |
//! | *SRE?    | Service Request Enable Query         | 10.35         |
//! | *STB     | Read Status Byte Query               | 10.36         |
//! | *TST     | Self-Test Query                      | 10.38         |
//! | *WAI     | Wait-To-Continue                     | 10.39         |
//!
//! Note that the comments about the default mandatory commands below are from the IEEE 488.2-1992 document and explain their purpose, not my implementation.

use crate::error::Result;
use crate::format::Character;
use crate::parameters::Arguments;
use crate::{both, prelude::*};
use crate::{nquery, qonly};

use super::IEEE488Device;

///## 10.3 *CLS, Clear Status Command
///> The Clear Status command clears status data structures, see 11.1.2, and forces the device to the Operation Complete
///> Command Idle State and the Operation Complete Query Idle State, see 12.5.2 and 12.5.3.
///>
///> If the Clear Status command immediately follows a <PROGRAM MESSAGE TERMINATOR>, the Output Queue
///> and the MAV bit will be cleared because any new <PROGRAM MESSAGE> after a <PROGRAM MESSAGE
///> TERMINATOR> clears the Output Queue, see 6.3.2.3.
pub struct ClsCommand;

impl<D> Command<D> for ClsCommand
where
    D: IEEE488Device,
{
    nquery!();

    fn event(&self, device: &mut D, _context: &mut Context, _args: Arguments) -> Result<()> {
        // Clear any device specific status
        device.exec_cls()
    }
}

///## 10.10 *ESE, Standard Event Status Enable Command
///> The Standard Event Status Enable command sets the Standard Event Status Enable Register bits as defined in 11.5.1.3.
///## 10.11 *ESE?, Standard Event Status Enable Query
///> The Standard Event Status Enable query allows the programmer to determine the current contents of the Standard
///> Event Status Enable Register. See 11.5.1.3.
pub struct EseCommand;

impl<D> Command<D> for EseCommand
where
    D: IEEE488Device,
{
    both!();

    fn event(&self, device: &mut D, _context: &mut Context, mut args: Arguments) -> Result<()> {
        let ese = args.data()?;
        device.set_ese(ese);
        Ok(())
    }

    fn query(
        &self,
        device: &mut D,
        _context: &mut Context,
        _args: Arguments,
        mut response: ResponseUnit,
    ) -> Result<()> {
        response.data(device.ese()).finish()
    }
}

///## 10.12 *ESR?, Standard Event Status Register Query
///> The Standard Event Status Register query allows the programmer to determine the current contents of the Standard
///> Event Status Register. Reading the Standard Event Status Register clears it. See 11.5.1.2.
pub struct EsrCommand;

impl<D> Command<D> for EsrCommand
where
    D: IEEE488Device,
{
    qonly!();

    fn query(
        &self,
        device: &mut D,
        _context: &mut Context,
        _args: Arguments,
        mut response: ResponseUnit,
    ) -> Result<()> {
        let esr = device.esr();
        device.set_esr(0);
        response.data(esr).finish()
    }
}

///## 10.14 *IDN?, Identification Query
///> The intent of the Identification query is for the unique identification of devices over the system interface.
///
///### 4.1.3.6 SCPI-99 Comments:
///> IEEE 488.2 is purposefully vague about the content of each of the four fields in the response
///> syntax. SCPI adds no further requirement, but here are some suggestions:
///>
///> All devices produced by a company should implement the *IDN? response consistently.
///>  * Field 1, the Manufacturer field, should be identical for all devices produced by a single company.
///>  * Field 2, the Model field, should NOT contain the word “MODEL”.
///>  * Field 4, the Firmware level field, should contain information about all separately revisable subsystems.
///> This information can be contained in single or multiple revision codes.
pub struct IdnCommand<'a> {
    pub manufacturer: &'a [u8],
    pub model: &'a [u8],
    pub serial: &'a [u8],
    pub firmware: &'a [u8],
}

impl<'a, D> Command<D> for IdnCommand<'a>
where
    D: IEEE488Device,
{
    qonly!();

    fn query(
        &self,
        _device: &mut D,
        _context: &mut Context,
        _args: Arguments,
        mut response: ResponseUnit,
    ) -> Result<()> {
        response
            .data(Character(self.manufacturer))
            .data(Character(self.model))
            .data(Character(self.serial))
            .data(Character(self.firmware))
            .finish()
    }
}

///## 10.18 *OPC, Operation Complete Command
///> The Operation Complete command causes the device to generate the operation complete message in the Standard
///> Event Status Register when all pending selected device operations have been finished. See 12.5.2.2 for details of
///> operation.
///## 10.19 *OPC?, Operation Complete Query
///> The Operation Complete query places an ASCII character "1" into the device's Output Queue when all pending
///> selected device operations have been finished. See 12.5.3 for details of operation.
///
pub struct OpcCommand;
impl<D> Command<D> for OpcCommand
where
    D: IEEE488Device,
{
    both!();

    fn event(&self, device: &mut D, _context: &mut Context, _args: Arguments) -> Result<()> {
        device.exec_opc()
    }

    fn query(
        &self,
        _device: &mut D,
        _context: &mut Context,
        _args: Arguments,
        mut response: ResponseUnit,
    ) -> Result<()> {
        response.data(true).finish()
    }
}

///## 10.32 *RST, Reset Command
///> The Reset command performs a device reset. The Reset command is the third level of reset in a three-level reset
///> strategy, see 17.1.2 and Appendix D. The Reset command shall do the following:
///>  * Except as explicitly excluded below, set the device-specific functions to a known state that is independent of
///> the past-use history of the device. Device-specific commands may be provided to program a different reset
///>  * state than the original factory-supplied one.
///>  * Set the macro defined by *DDT to a device-defined state, see 10.4.
///>  * Disable macros, see 10.8.
///>  * Force the device into the OCIS state, see 12.5.2.
///>  * Force the device into the OQIS state, see 12.5.3.
///> The reset command explicitly shall NOT affect the following:
///>  * The state of the IEEE 488.1 interface.
///>  * The selected IEEE 488.1 address of the device.
///>  * The Output Queue.
///>  * Any Event Enable Register setting, including the Standard Event Status Enable Register settings, see
///> 11.4.2.3.4 and 11.5.1.3.4.
///>  * Any Event Register setting, including the Standard Event Status Register settings, see 11.4.2.2.4 and
///> 11.5.1.2.4.
///>  * The power-on-status-clear flag setting.
///>  * Macros defined with the DeÞne Macro Contents command.
///>  * Calibration data that affects device specifications.
///>  * The Protected User Data query response.
///>  * The Resource Description Transfer query response.
///>  * The Service Request Enable Register setting, see 11.3.2.4.
///>  * The Parallel Poll Enable Register setting, see 11.6.1.4.
///>  * The memory register(s) associated with *SAV.
///> The scope of the *LRN? response and *RCL (if implemented) is the same as *RST. See 10.17.3 and 10.29.3.
pub struct RstCommand;
impl<D> Command<D> for RstCommand
where
    D: IEEE488Device,
{
    nquery!();

    fn event(&self, device: &mut D, _context: &mut Context, _args: Arguments) -> Result<()> {
        device.exec_rst()
    }
}

///## 10.34 *SRE, Service Request Enable Command
///> The Service Request Enable command sets the Service Request Enable Register bits as defined in 11.3.2.
///## 10.35 *SRE?, Service Request Enable Query
///> The Service Request Enable query allows the programmer to determine the current contents of the Service Request
///> Enable Register, see 11.3.2.
pub struct SreCommand;
impl<D> Command<D> for SreCommand
where
    D: IEEE488Device,
{
    both!();

    fn event(&self, device: &mut D, _context: &mut Context, mut args: Arguments) -> Result<()> {
        let sre = args.data()?;
        device.set_sre(sre);
        Ok(())
    }

    fn query(
        &self,
        device: &mut D,
        _context: &mut Context,
        _args: Arguments,
        mut response: ResponseUnit,
    ) -> Result<()> {
        response.data(device.sre()).finish()
    }
}

///## 10.36 *STB?, Read Status Byte Query
///> The Read Status Byte query allows the programmer to read the status byte and Master Summary Status bit.
pub struct StbCommand;
impl<D> Command<D> for StbCommand
where
    D: IEEE488Device,
{
    qonly!();

    fn query(
        &self,
        device: &mut D,
        context: &mut Context,
        _args: Arguments,
        mut response: ResponseUnit,
    ) -> Result<()> {
        // MAV is provided by the context from whatever interface the command was received on
        let mut stb = device.read_stb();
        if context.mav {
            stb |= 0x10;
        }
        response.data(stb).finish()
    }
}

///## 10.38 *TST?, Self-Test Query
///> The self-test query causes an internal self-test and places a response into the Output Queue indicating whether or not
///> the device completed the self-test without any detected errors. Optionally, information on why the self-test was not
///> completed may be contained in the response. The scope of the internal self-test shall appear in the device
///> documentation, see 4.9.
///>
///> The *TST? query shall not require any local operator interaction. It shall not create bus conditions that are violations
///> to the IEEE Std 488.1-1987 or IEEE Std 488.2-1992 standards. Otherwise, the scope of the self-test is completely
///> at the discretion of the device designer.
///>
///> Upon successful completion of *TST?, the device settings shall be restored to their values prior to the *TST?; set to
///> fixed, known values that are stated in the device documentation; or set to values deÞned by the user and stored in local
///> memory.
pub struct TstCommand;
impl<D> Command<D> for TstCommand
where
    D: IEEE488Device,
{
    qonly!();

    fn query(
        &self,
        device: &mut D,
        _context: &mut Context,
        _args: Arguments,
        mut response: ResponseUnit,
    ) -> Result<()> {
        response
            .data(
                device
                    .exec_tst()
                    .map(|_| 0i16)
                    .unwrap_or_else(|err| err.get_code()),
            )
            .finish()
    }
}

///## 10.39 *WAI, Wait-to-Continue Command
///> The Wait-to-Continue command shall prevent the device from executing any further commands or queries until the no-
///> operation-pending flag is TRUE. See 12.5.1.
///>
///> NOTE - In a device that implements only sequential commands, the no-operation-pending flag is always TRUE
pub struct WaiCommand;
impl<D> Command<D> for WaiCommand
where
    D: IEEE488Device,
{
    nquery!();
    fn event(&self, _device: &mut D, _context: &mut Context, _args: Arguments) -> Result<()> {
        Ok(())
    }
}

#[macro_export]
macro_rules! ieee488_idn {
    ($manufacturer:expr, $model:expr, $serial:expr, $firmware:expr) => {
        $crate::prelude::Leaf {
            name: b"*IDN",
            default: false,
            handler: &$crate::ieee488::mandatory::IdnCommand {
                manufacturer: $manufacturer,
                model: $model,
                serial: $serial,
                firmware: $firmware,
            },
        }
    };
}

#[macro_export]
macro_rules! ieee488_cls {
    () => {
        $crate::prelude::Leaf {
            name: b"*CLS",
            default: false,
            handler: &$crate::ieee488::mandatory::ClsCommand,
        }
    };
}

#[macro_export]
macro_rules! ieee488_ese {
    () => {
        $crate::prelude::Leaf {
            name: b"*ESE",
            default: false,
            handler: &$crate::ieee488::mandatory::EseCommand,
        }
    };
}

#[macro_export]
macro_rules! ieee488_esr {
    () => {
        $crate::prelude::Leaf {
            name: b"*ESR",
            default: false,
            handler: &$crate::ieee488::mandatory::EsrCommand,
        }
    };
}

#[macro_export]
macro_rules! ieee488_opc {
    () => {
        $crate::prelude::Leaf {
            name: b"*OPC",
            default: false,
            handler: &$crate::ieee488::mandatory::OpcCommand,
        }
    };
}

#[macro_export]
macro_rules! ieee488_rst {
    () => {
        $crate::prelude::Leaf {
            name: b"*RST",
            default: false,
            handler: &$crate::ieee488::mandatory::RstCommand,
        }
    };
}

#[macro_export]
macro_rules! ieee488_sre {
    () => {
        $crate::prelude::Leaf {
            name: b"*SRE",
            default: false,
            handler: &$crate::ieee488::mandatory::SreCommand,
        }
    };
}

#[macro_export]
macro_rules! ieee488_stb {
    () => {
        $crate::prelude::Leaf {
            name: b"*STB",
            default: false,
            handler: &$crate::ieee488::mandatory::StbCommand,
        }
    };
}

#[macro_export]
macro_rules! ieee488_tst {
    () => {
        $crate::prelude::Leaf {
            name: b"*TST",
            default: false,
            handler: &$crate::ieee488::mandatory::TstCommand,
        }
    };
}

#[macro_export]
macro_rules! ieee488_wai {
    () => {
        $crate::prelude::Leaf {
            name: b"*WAI",
            default: false,
            handler: &$crate::ieee488::mandatory::WaiCommand,
        }
    };
}
