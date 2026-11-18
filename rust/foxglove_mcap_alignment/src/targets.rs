use crate::{Episode, Error, ErrorKind, MetadataSource, OrderedSource};

/// Positive, reduced rational Hz, at most one target per nanosecond.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rate {
    numerator: u64,
    denominator: u64,
}

impl Rate {
    pub fn new(numerator: u64, denominator: u64) -> Result<Self, Error> {
        if numerator == 0
            || denominator == 0
            || u128::from(numerator) > 1_000_000_000 * u128::from(denominator)
        {
            return Err(Error::new(
                ErrorKind::InvalidConfiguration,
                "rate must be positive and at most one tick/ns",
            ));
        }
        let mut a = numerator;
        let mut b = denominator;
        while b != 0 {
            let remainder = a % b;
            a = b;
            b = remainder;
        }
        Ok(Self {
            numerator: numerator / a,
            denominator: denominator / a,
        })
    }
    pub fn numerator(self) -> u64 {
        self.numerator
    }
    pub fn denominator(self) -> u64 {
        self.denominator
    }
}

/// Fixed grid phase or one explicitly resolved reference channel.
pub enum Timeline<'a> {
    Fixed { rate: Rate, origin_ns: u64 },
    Reference(Box<dyn MetadataSource + 'a>),
}

struct Grid {
    origin: u64,
    rate: Rate,
    tick: i128,
}

fn overflow() -> Error {
    Error::new(
        ErrorKind::ArithmeticOverflow,
        "rational target arithmetic overflow",
    )
}

impl Grid {
    fn new(episode: Episode, origin: u64, rate: Rate) -> Result<Self, Error> {
        // ceil(ideal) >= start iff ideal > start - 1. Use unsigned magnitude
        // multiplication to cover the full u64 timestamp/rate domain in u128.
        let delta = i128::from(episode.start) - 1 - i128::from(origin);
        let product = delta
            .unsigned_abs()
            .checked_mul(u128::from(rate.numerator))
            .ok_or_else(overflow)?;
        let divisor = 1_000_000_000 * u128::from(rate.denominator);
        let quotient = i128::try_from(product / divisor).map_err(|_| overflow())?;
        let floor = if delta < 0 {
            -quotient - i128::from(product % divisor != 0)
        } else {
            quotient
        };
        Ok(Self {
            origin,
            rate,
            tick: floor.checked_add(1).ok_or_else(overflow)?,
        })
    }
    fn next(&mut self, episode: Episode) -> Result<Option<u64>, Error> {
        let period = 1_000_000_000 * u128::from(self.rate.denominator);
        let divisor = u128::from(self.rate.numerator);
        // Split the period before multiplying: even the first out-of-episode
        // tick must not overflow a widened intermediate near u64::MAX.
        let tick = self.tick.unsigned_abs();
        let fractional = tick.checked_mul(period % divisor).ok_or_else(overflow)?;
        let whole = tick.checked_mul(period / divisor).ok_or_else(overflow)?;
        let quotient = i128::try_from(
            whole
                .checked_add(fractional / divisor)
                .ok_or_else(overflow)?,
        )
        .map_err(|_| overflow())?;
        let offset = if self.tick < 0 {
            -quotient
        } else {
            quotient
                .checked_add(i128::from(fractional % divisor != 0))
                .ok_or_else(overflow)?
        };
        let time = i128::from(self.origin)
            .checked_add(offset)
            .ok_or_else(overflow)?;
        if time >= i128::from(episode.end) {
            return Ok(None);
        }
        let time = u64::try_from(time).map_err(|_| overflow())?;
        if time < episode.start {
            return Err(overflow());
        }
        self.tick = self.tick.checked_add(1).ok_or_else(overflow)?;
        Ok(Some(time))
    }
}

enum Generator<'a> {
    Fixed(Grid),
    Reference {
        source: OrderedSource<'a>,
        previous: Option<u64>,
    },
}

impl Generator<'_> {
    fn next(&mut self, episode: Episode) -> Result<Option<u64>, Error> {
        match self {
            Self::Fixed(grid) => grid.next(episode),
            Self::Reference { source, previous } => {
                let Some(record) = source.next_record()? else {
                    return Ok(None);
                };
                if *previous == Some(record.log_time) {
                    return Err(Error::new(
                        ErrorKind::DuplicateReference,
                        "repeated in-episode reference timestamp",
                    ));
                }
                *previous = Some(record.log_time);
                Ok(Some(record.log_time))
            }
        }
    }
}

/// Streaming target times, rejecting job emptiness at construction.
pub struct Targets<'a> {
    generator: Generator<'a>,
    episode: Episode,
    first: Option<u64>,
    ended: bool,
}

impl<'a> Targets<'a> {
    pub fn new(episode: Episode, timeline: Timeline<'a>) -> Result<Self, Error> {
        let mut generator = match timeline {
            Timeline::Fixed { rate, origin_ns } => {
                Generator::Fixed(Grid::new(episode, origin_ns, rate)?)
            }
            Timeline::Reference(source) => {
                let mut ordered = OrderedSource::new(source, episode);
                let mut previous = None;
                while let Some(record) = ordered.next_record()? {
                    if previous == Some(record.log_time) {
                        return Err(Error::new(
                            ErrorKind::DuplicateReference,
                            "repeated in-episode reference timestamp",
                        ));
                    }
                    previous = Some(record.log_time);
                }
                ordered.source.rewind()?;
                Generator::Reference {
                    source: OrderedSource::new(ordered.source, episode),
                    previous: None,
                }
            }
        };
        let first = generator
            .next(episode)?
            .ok_or_else(|| Error::new(ErrorKind::EmptyTimeline, "no target in episode"))?;
        Ok(Self {
            generator,
            episode,
            first: Some(first),
            ended: false,
        })
    }

    pub fn next_time(&mut self) -> Result<Option<u64>, Error> {
        if self.ended {
            return Ok(None);
        }
        if self.first.is_some() {
            return Ok(self.first.take());
        }
        let next = self.generator.next(self.episode);
        if !matches!(next, Ok(Some(_))) {
            self.ended = true;
        }
        next
    }
}
