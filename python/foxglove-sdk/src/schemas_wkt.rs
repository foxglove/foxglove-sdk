//! Wrappers for well-known types.

use pyo3::exceptions::PyOverflowError;
use pyo3::prelude::*;
use pyo3::types::{timezone_utc, PyDateTime};

/// A timestamp in seconds and nanoseconds
///
/// :param sec: The number of seconds since a user-defined epoch.
/// :param nsec: The number of nanoseconds since the :py:attr:\`sec\` value.
#[pyclass(module = "foxglove.schemas", eq)]
#[derive(Clone, PartialEq)]
pub struct Timestamp(pub(crate) foxglove::schemas::Timestamp);

#[pymethods]
impl Timestamp {
    #[new]
    #[pyo3(signature = (sec, nsec=None))]
    fn new(sec: u32, nsec: Option<u32>) -> Self {
        let nsec = nsec.unwrap_or(0);
        Self(foxglove::schemas::Timestamp { sec, nsec })
    }

    fn __repr__(&self) -> String {
        format!("Timestamp(sec={}, nsec={})", self.0.sec, self.0.nsec).to_string()
    }

    /// Creates a :py:class:`Timestamp` from an epoch timestamp, such as is returned by
    /// :py:func:`time.time` or :py:func:`datetime.datetime.timestamp`.
    ///
    /// Raises `OverflowError` if the timestamp cannot be represented.
    ///
    /// :param timestamp: Seconds since epoch
    /// :type timestamp: float
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
    /// :type dt: :py:class:`datetime.datetime`
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
        if days < 0 {
            return Err(PyOverflowError::new_err("timestamp out of range"));
        }
        let Some(sec) = (days as u32)
            .checked_mul(24 * 3600)
            .and_then(|s| s.checked_add(seconds))
        else {
            return Err(PyOverflowError::new_err("timestamp out of range"));
        };
        Ok(Self::new(sec, Some(microseconds * 1000)))
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
#[derive(Clone, PartialEq)]
pub struct Duration(pub(crate) foxglove::schemas::Duration);

#[pymethods]
impl Duration {
    #[new]
    #[pyo3(signature = (sec, nsec=None))]
    fn new(sec: i32, nsec: Option<u32>) -> Self {
        let nsec = nsec.unwrap_or(0);
        Self(foxglove::schemas::Duration { sec, nsec })
    }

    fn __repr__(&self) -> String {
        format!("Duration(sec={}, nsec={})", self.0.sec, self.0.nsec).to_string()
    }

    /// Creates a :py:class:`Duration` from seconds.
    ///
    /// Raises `OverflowError` if the duration cannot be represented.
    ///
    /// :param secs: Seconds
    /// :type secs: float
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
    /// :type td: :py:class:`datetime.timedelta`
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
        Ok(Self::new(sec, Some(microseconds * 1000)))
    }
}

impl From<Duration> for foxglove::schemas::Duration {
    fn from(value: Duration) -> Self {
        value.0
    }
}
