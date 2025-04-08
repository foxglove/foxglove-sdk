//! Wrappers for well-known types.

use pyo3::exceptions::PyOverflowError;
use pyo3::prelude::*;
use pyo3::types::{timezone_utc, PyDateTime};

/// A timestamp in seconds and nanoseconds
///
/// :param sec: The number of seconds since a user-defined epoch.
/// :param nsec: The number of nanoseconds since the sec value.
#[pyclass(module = "foxglove.schemas", eq)]
#[derive(Clone, PartialEq, Eq)]
pub struct Timestamp(foxglove::schemas::Timestamp);

#[pymethods]
impl Timestamp {
    #[new]
    #[pyo3(signature = (sec, nsec=None))]
    fn new(sec: u32, nsec: Option<u32>) -> PyResult<Self> {
        let timestamp = foxglove::schemas::Timestamp::new_checked(sec, nsec.unwrap_or(0))
            .ok_or_else(|| PyOverflowError::new_err("timestamp out of range"))?;
        Ok(Self(timestamp))
    }

    fn __repr__(&self) -> String {
        format!("Timestamp(sec={}, nsec={})", self.sec(), self.nsec()).to_string()
    }

    /// The number of seconds in the timestamp.
    #[getter]
    fn sec(&self) -> u32 {
        self.0.sec()
    }

    /// The number of fractional seconds in the timestamp, as nanoseconds.
    #[getter]
    fn nsec(&self) -> u32 {
        self.0.nsec()
    }

    /// Creates a :py:class:`Timestamp` from the current system time.
    ///
    /// Raises `OverflowError` if the timestamp cannot be represented.
    ///
    /// :rtype: :py:class:`Timestamp`
    #[staticmethod]
    #[pyo3(signature = ())]
    fn now() -> PyResult<Self> {
        let now = std::time::SystemTime::now();
        Ok(Self(foxglove::schemas::Timestamp::try_from(now).map_err(
            |_| PyOverflowError::new_err("timestamp out of range"),
        )?))
    }

    /// Creates a :py:class:`Timestamp` from an epoch timestamp, such as is returned by
    /// :py:func:`!time.time` or :py:func:`!datetime.datetime.timestamp`.
    ///
    /// Raises `OverflowError` if the timestamp cannot be represented.
    ///
    /// :param timestamp: Seconds since epoch
    /// :rtype: :py:class:`Timestamp`
    #[staticmethod]
    #[pyo3(signature = (timestamp))]
    fn from_epoch_secs(timestamp: f64) -> PyResult<Self> {
        foxglove::schemas::Timestamp::try_from_epoch_secs_f64(timestamp)
            .map(Self)
            .map_err(|_| PyOverflowError::new_err("timestamp out of range"))
    }

    /// Creates a UNIX epoch :py:class:`Timestamp` from a datetime object.
    ///
    /// Naive datetime objects are presumed to be in the local timezone.
    ///
    /// Raises `OverflowError` if the timestamp cannot be represented.
    ///
    /// :param dt: Datetime
    /// :rtype: :py:class:`Timestamp`
    #[staticmethod]
    #[pyo3(signature = (dt))]
    fn from_datetime(py: Python, mut dt: Py<PyDateTime>) -> PyResult<Self> {
        // If this is a naive datetime, presume the local timezone.
        let tzinfo: Py<PyAny> = dt.getattr(py, "tzinfo")?;
        if tzinfo.is_none(py) {
            dt = dt.call_method0(py, "astimezone")?.extract(py)?;
        }
        let utc = timezone_utc(py);

        // We're not simply using `datetime.total_seconds()`, because the conversion to floating
        // point loses preceision and is not round-trippable.
        let epoch = PyDateTime::new(py, 1970, 1, 1, 0, 0, 0, 0, Some(&utc)).unwrap();
        let td = dt.call_method1(py, "__sub__", (epoch,))?;

        // Timedelta objects are normalized:
        //
        // - 0 <= microseconds < 1000000
        // - 0 <= seconds < 3600*24 (the number of seconds in one day)
        // - -999999999 <= days <= 999999999
        //
        // It is safe to multiply microseconds by 1000.
        let days: i32 = td.getattr(py, "days")?.extract(py)?;
        let seconds: u32 = td.getattr(py, "seconds")?.extract(py)?;
        let microseconds: u32 = td.getattr(py, "microseconds")?.extract(py)?;
        let Ok(days) = u32::try_from(days) else {
            return Err(PyOverflowError::new_err("timestamp out of range"));
        };
        let Some(sec) = days
            .checked_mul(24 * 3600)
            .and_then(|s| s.checked_add(seconds))
        else {
            return Err(PyOverflowError::new_err("timestamp out of range"));
        };
        Self::new(sec, Some(microseconds * 1000))
    }
}

impl From<Timestamp> for foxglove::schemas::Timestamp {
    fn from(value: Timestamp) -> Self {
        value.0
    }
}

/// A duration, composed of seconds and nanoseconds
///
/// :param sec: The number of seconds in the duration.
/// :param nsec: The number of nanoseconds in the positive direction.
#[pyclass(module = "foxglove.schemas", eq)]
#[derive(Clone, PartialEq, Eq)]
pub struct Duration(foxglove::schemas::Duration);

#[pymethods]
impl Duration {
    #[new]
    #[pyo3(signature = (sec, nsec=None))]
    fn new(sec: i32, nsec: Option<u32>) -> PyResult<Self> {
        let duration = foxglove::schemas::Duration::new_checked(sec, nsec.unwrap_or(0))
            .ok_or_else(|| PyOverflowError::new_err("duration out of range"))?;
        Ok(Self(duration))
    }

    fn __repr__(&self) -> String {
        format!("Duration(sec={}, nsec={})", self.sec(), self.nsec()).to_string()
    }

    /// The number of seconds in the duration.
    #[getter]
    fn sec(&self) -> i32 {
        self.0.sec()
    }

    /// The number of fractional seconds in the duration, as nanoseconds.
    #[getter]
    fn nsec(&self) -> u32 {
        self.0.nsec()
    }

    /// Creates a :py:class:`Duration` from seconds.
    ///
    /// Raises `OverflowError` if the duration cannot be represented.
    ///
    /// :param secs: Seconds
    /// :rtype: :py:class:`Duration`
    #[staticmethod]
    #[pyo3(signature = (secs))]
    fn from_secs(secs: f64) -> PyResult<Self> {
        foxglove::schemas::Duration::try_from_secs_f64(secs)
            .map(Self)
            .map_err(|_| PyOverflowError::new_err("duration out of range"))
    }

    /// Creates a :py:class:`Duration` from a timedelta.
    ///
    /// Raises `OverflowError` if the duration cannot be represented.
    ///
    /// :param td: Timedelta
    /// :rtype: :py:class:`Duration`
    #[staticmethod]
    #[pyo3(signature = (td))]
    fn from_timedelta(py: Python, td: Py<PyAny>) -> PyResult<Self> {
        // Timedelta objects are normalized:
        //
        // - 0 <= microseconds < 1000000
        // - 0 <= seconds < 3600*24 (the number of seconds in one day)
        // - -999999999 <= days <= 999999999
        //
        // It is safe to read seconds as i32, and multiply microseconds by 1000.
        let microseconds: u32 = td.getattr(py, "microseconds")?.extract(py)?;
        let seconds: i32 = td.getattr(py, "seconds")?.extract(py)?;
        let days: i32 = td.getattr(py, "days")?.extract(py)?;
        let Some(sec) = days
            .checked_mul(3600 * 24)
            .and_then(|s| s.checked_add(seconds))
        else {
            return Err(PyOverflowError::new_err("duration out of range"));
        };
        Self::new(sec, Some(microseconds * 1000))
    }
}

impl From<Duration> for foxglove::schemas::Duration {
    fn from(value: Duration) -> Self {
        value.0
    }
}
