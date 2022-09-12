use std::fmt::{Debug, Display, Formatter};

#[repr(C)]
pub enum Error {
	EarlierThanUnixEpoch,
	InstantAdd,
	CreateInstant
}
impl Display for Error{
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		match self {
			Error::EarlierThanUnixEpoch => write!(f,"The time gotten was earlier than the Unix epoch(0:0:0 1.1.1971)"),
			Error::InstantAdd => write!(f,"There was an error adding a Duration to a Instant"),
			Error::CreateInstant => write!(f,"There was an error creating a Instant"),
		}
		
	}
}
impl Debug for Error{
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(&self,f)
	}
}

impl std::error::Error for Error {
}