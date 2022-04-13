use std::fmt;

#[derive(Debug)]
pub enum ReleasrError {
    IoError(std::io::Error),
    ParseError(serde_yaml::Error),
    CustomError(String),
}

impl std::error::Error for ReleasrError {}

impl fmt::Display for ReleasrError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ReleasrError::CustomError(error) => write!(f, "{}", error),
            ReleasrError::ParseError(error) => write!(f, "error reading file: {}", error),
            ReleasrError::IoError(err) => write!(f, "io error: {}", err)
        }
    }
}

impl From<std::io::Error> for ReleasrError {
    fn from(err: std::io::Error) -> Self {
        ReleasrError::IoError(err)
    }
}

impl From<serde_yaml::Error> for ReleasrError {
    fn from(err: serde_yaml::Error) -> Self {
        ReleasrError::ParseError(err)
    }
}