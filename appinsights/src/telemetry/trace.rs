use chrono::{DateTime, Utc};

use crate::telemetry::{ContextTags, Measurements, Properties, Telemetry};

// Represents printf-like trace statements that can be text searched.
pub struct TraceTelemetry {
    /// A trace message.
    message: String,

    // Severity level.
    severity: SeverityLevel,

    /// The time stamp when this telemetry was measured.
    timestamp: DateTime<Utc>,

    /// Custom properties.
    properties: Properties,

    /// Telemetry context containing extra, optional tags.
    tags: ContextTags,
}

impl TraceTelemetry {
    /// Creates an event telemetry item with specified name.
    pub fn new(message: &str, severity: SeverityLevel) -> Self {
        Self {
            message: message.into(),
            severity,
            timestamp: Utc::now(),
            properties: Default::default(),
            tags: Default::default(),
        }
    }
}

impl Telemetry for TraceTelemetry {
    /// Returns the time when this telemetry was measured.
    fn timestamp(&self) -> &DateTime<Utc> {
        &self.timestamp
    }

    /// Returns custom properties to submit with the telemetry item.
    fn properties(&self) -> &Properties {
        &self.properties
    }

    /// Returns None always. No measurements available for trace telemetry items.
    fn measurements(&self) -> Option<&Measurements> {
        None
    }

    /// Returns context data containing extra, optional tags. Overrides values found on client telemetry context.
    fn tags(&self) -> &ContextTags {
        &self.tags
    }
}

/// Defines the level of severity for the event.
pub enum SeverityLevel {
    Verbose,
    Information,
    Warning,
    Error,
    Critical,
}