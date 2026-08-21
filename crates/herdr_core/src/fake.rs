use std::collections::{BTreeMap, VecDeque};

use crate::transport::{
    AttachmentId, HerdrTransport, TransportCommand, TransportError, TransportNotification,
};

#[derive(Default)]
pub(crate) struct FakeHerdrTransport {
    incoming: VecDeque<TransportNotification>,
    input_by_attachment: BTreeMap<AttachmentId, Vec<Vec<u8>>>,
}

impl FakeHerdrTransport {
    pub(crate) fn push_notification(&mut self, notification: TransportNotification) {
        self.incoming.push_back(notification);
    }

    pub(crate) fn inputs_for(&self, attachment: AttachmentId) -> &[Vec<u8>] {
        self.input_by_attachment
            .get(&attachment)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
}

impl HerdrTransport for FakeHerdrTransport {
    fn send(&mut self, command: TransportCommand) -> Result<(), TransportError> {
        if let TransportCommand::WriteInput(input) = &command {
            self.input_by_attachment
                .entry(input.attachment)
                .or_default()
                .push(input.bytes.clone());
        }
        Ok(())
    }

    fn try_recv(&mut self) -> Result<Option<TransportNotification>, TransportError> {
        Ok(self.incoming.pop_front())
    }
}
