use crate::{command::Command, nquery, scpi1999::ScpiDevice};

use super::Trigger;

pub trait Abort {
    /// Abort trigger
    fn abort(&mut self);
}

/// # ABORt
/// `ABORt`
///
/// The ABORt command resets the trigger system and places all trigger sequences in the IDLE
/// state. Any actions related to the trigger system that are in progress, such as a sweep or
/// acquiring a measurement, shall also be aborted as quickly as possible. The ABORt command
/// shall not be considered complete until all trigger sequences are in the IDLE state. The
/// execution of an ABORt command shall set false the pending operation flags that were set by
/// the initiation of the trigger system.
///
/// This command is an event and has no associated *RST condition or query form.
struct AbortCommand;

impl<D> Command<D> for AbortCommand
where
    D: ScpiDevice + Trigger,
{
    nquery!();

    fn event(
        &self,
        device: &mut D,
        _context: &mut crate::Context,
        _args: crate::prelude::Arguments,
    ) -> crate::error::Result<()> {
        device.abort();
        Ok(())
    }
}
