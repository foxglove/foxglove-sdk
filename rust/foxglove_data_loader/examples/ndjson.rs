//! This example data loader parses newline-separated lines of JSON of 3 forms:
//!
//! {"type":"info","start":0,"end":30,"sphere_count":360,"temperature_count":120}
//! {"type":"temperature","time":0,"ambient":21,"cpu0":70,"cpu1":65,"cpu2":68,"cpu3":72}
//! {"type":"sphere","id":"A","time":0,"x":0,"y":2,"z":1.5}
//!
//! The loader stores the records in memory and puts the sphere data into a SceneUpdate topic called
//! scene and the temperature rows go into a topic called /temperature.

#![no_main]
use anyhow::{anyhow, bail};
use foxglove::schemas::{
    Color, Pose, Quaternion, SceneEntity, SceneUpdate, SpherePrimitive, Timestamp, Vector3,
};
use foxglove::Encode;
use prost::Message as ProstMessage;
use std::{
    cell::RefCell,
    collections::BTreeSet,
    io::{BufRead, BufReader},
    rc::Rc,
};

foxglove_data_loader::export!(NDJsonLoader);
use foxglove_data_loader::{
    console, reader, BackfillArgs, Channel, DataLoader, DataLoaderArgs, Initialization, Message,
    MessageIterator, MessageIteratorArgs, Schema, TimeRange,
};

const SPHERE_CHANNEL_ID: u16 = 1;
const SPHERE_SCHEMA_ID: u16 = 1;
const TEMPERATURE_CHANNEL_ID: u16 = 2;
const TEMPERATURE_SCHEMA_ID: u16 = 2;

#[derive(Default)]
struct NDJsonLoader {
    path: String,
    rows: Rc<RefCell<Vec<Row>>>,
}

impl DataLoader for NDJsonLoader {
    type MessageIterator = NDJsonIterator;
    type Error = anyhow::Error;

    fn new(args: DataLoaderArgs) -> Self {
        let path = args
            .paths
            .first()
            .unwrap_or_else(|| panic!["didn't receive a file path as input"])
            .clone();
        Self {
            path,
            ..Self::default()
        }
    }

    fn initialize(&self) -> Result<Initialization, Self::Error> {
        let lines = BufReader::new(reader::open(&self.path)).lines();
        let mut rows: Vec<Row> = lines
            .map(|rline| {
                rline
                    .and_then(|line| serde_json::from_str(&line).map_err(|err| err.into()))
                    .map_err(|err| err.into())
            })
            .collect::<Result<Vec<Row>, Self::Error>>()?;
        rows.sort_by(|a, b| {
            let time_a = a.get_time().unwrap_or(f64::MIN);
            let time_b = b.get_time().unwrap_or(f64::MIN);
            f64::partial_cmp(&time_a, &time_b).expect("time comparison failed")
        });

        let Some(Row::Info(info)) = rows.iter().find(|row| matches![row, Row::Info(_)]).cloned()
        else {
            bail!["did not find type=\"info\" row"]
        };
        self.rows.replace(rows);
        console::log(&format!["info={info:?}"]);

        let sphere_channel = Channel {
            id: SPHERE_CHANNEL_ID,
            schema_id: Some(SPHERE_SCHEMA_ID),
            topic_name: "/scene".to_string(),
            message_encoding: SceneUpdate::get_message_encoding(),
            message_count: Some(info.sphere_count),
        };
        let sphere_schema = Schema::from_id_sdk(
            SPHERE_SCHEMA_ID,
            SceneUpdate::get_schema().ok_or(anyhow!["failed to get SceneUpdate schema"])?,
        );

        let temperature_channel = Channel {
            id: TEMPERATURE_CHANNEL_ID,
            schema_id: Some(TEMPERATURE_SCHEMA_ID),
            topic_name: "/temperature".to_string(),
            message_encoding: "json".to_string(),
            message_count: Some(info.temperature_count),
        };

        let temperature_schema = Schema {
            id: TEMPERATURE_SCHEMA_ID,
            name: "temperature".to_string(),
            encoding: "jsonschema".to_string(),
            data: TEMPERATURE_SCHEMA.to_vec(),
        };

        let time_range = TimeRange {
            start_time: seconds_to_nanos(info.start),
            end_time: seconds_to_nanos(info.end),
        };

        Ok(Initialization {
            channels: vec![sphere_channel, temperature_channel],
            schemas: vec![sphere_schema, temperature_schema],
            time_range,
            problems: vec![],
        })
    }

    fn create_iter(&self, args: MessageIteratorArgs) -> Result<Self::MessageIterator, Self::Error> {
        Ok(NDJsonIterator::open(self.rows.clone(), &args))
    }

    fn get_backfill(&self, args: BackfillArgs) -> Result<Vec<Message>, Self::Error> {
        let want_sphere = args.channels.contains(&SPHERE_CHANNEL_ID);
        let want_temperature = args.channels.contains(&TEMPERATURE_CHANNEL_ID);

        let rows = self.rows.borrow();
        let mut backfill: Vec<Message> = vec![];
        if want_sphere {
            let option_backfill_sphere = rows
                .iter()
                .take_while(|row| match row {
                    Row::Sphere(sphere) => seconds_to_nanos(sphere.time) < args.time,
                    _ => false,
                })
                .last();
            if let Some(Row::Sphere(sphere)) = option_backfill_sphere {
                backfill.push(sphere.into());
            }
        }
        if want_temperature {
            let option_backfill_temperature = rows
                .iter()
                .take_while(|row| match row {
                    Row::Temperature(temperature) => seconds_to_nanos(temperature.time) < args.time,
                    _ => false,
                })
                .last();
            if let Some(Row::Temperature(temperature)) = option_backfill_temperature {
                backfill.push(temperature.into());
            }
        }
        Ok(backfill)
    }
}

struct NDJsonIterator {
    rows: Rc<RefCell<Vec<Row>>>,
    index: RefCell<usize>,
    start: u64,
    end: u64,
    channels: BTreeSet<u16>,
}

impl NDJsonIterator {
    fn open(rows: Rc<RefCell<Vec<Row>>>, args: &MessageIteratorArgs) -> Self {
        Self {
            rows: rows.clone(),
            index: 0.into(),
            start: args.start_time.unwrap_or(0),
            end: args.end_time.unwrap_or(u64::MAX),
            channels: args.channels.iter().copied().collect(),
        }
    }
}

impl MessageIterator for NDJsonIterator {
    type Error = anyhow::Error;

    fn next(&self) -> Option<Result<Message, Self::Error>> {
        loop {
            let index = *self.index.borrow();
            self.index.replace(index + 1);
            let rows = self.rows.borrow();
            let row = rows.get(index);
            if let Some(time) = row.and_then(|r| r.get_time().map(seconds_to_nanos)) {
                if time < self.start {
                    continue;
                }
                if time > self.end {
                    return None;
                }
            };
            match row {
                None => return None,
                Some(Row::Info(_)) => {}
                Some(Row::Sphere(sphere)) => {
                    if self.channels.contains(&SPHERE_CHANNEL_ID) {
                        return Some(Ok(sphere.into()));
                    }
                }
                Some(Row::Temperature(temperature)) => {
                    if self.channels.contains(&TEMPERATURE_CHANNEL_ID) {
                        return Some(Ok(temperature.into()));
                    }
                }
            };
        }
    }
}

// floating point time in seconds to u64 nanoseconds
fn seconds_to_nanos(time: f64) -> u64 {
    (time * 1.0e9) as u64
}

fn get_timestamp(time: f64) -> Timestamp {
    let time_nanos = seconds_to_nanos(time);
    Timestamp::new(
        (time_nanos / 1_000_000_000) as u32,
        (time_nanos % 1_000_000_000) as u32,
    )
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
enum Row {
    #[serde(rename = "info")]
    Info(Info),
    #[serde(rename = "sphere")]
    Sphere(Sphere),
    #[serde(rename = "temperature")]
    Temperature(Temperature),
}

impl Row {
    fn get_time(&self) -> Option<f64> {
        match self {
            Row::Info(_) => None,
            Row::Sphere(sphere) => Some(sphere.time),
            Row::Temperature(temperature) => Some(temperature.time),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Info {
    start: f64,
    end: f64,
    sphere_count: u64,
    temperature_count: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Sphere {
    id: String,
    time: f64,
    x: f64,
    y: f64,
    z: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Temperature {
    time: f64,
    ambient: f64,
    cpu0: f64,
    cpu1: f64,
    cpu2: f64,
    cpu3: f64,
}

const TEMPERATURE_SCHEMA: &[u8] = br#"{
    "title": "CpuTemperatures",
    "type": "object",
    "properties": {
        "time": {
            "description": "time of measurement",
            "type": "number"
        },
        "ambient": {
            "description": "ambient temperature (celsius)",
            "type": "number"
        },
        "cpu0": {
            "description": "cpu0 temperature (celsius)",
            "type": "number"
        },
        "cpu1": {
            "description": "cpu1 temperature (celsius)",
            "type": "number"
        },
        "cpu2": {
            "description": "cpu2 temperature (celsius)",
            "type": "number"
        },
        "cpu3": {
            "description": "cpu3 temperature (celsius)",
            "type": "number"
        }
    }
}"#;

impl From<&Sphere> for Message {
    fn from(sphere: &Sphere) -> Message {
        let sphere_primitive = SpherePrimitive {
            pose: Some(Pose {
                position: Some(Vector3 {
                    x: sphere.x,
                    y: sphere.y,
                    z: sphere.z,
                }),
                orientation: Some(Quaternion {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    w: 1.0,
                }),
            }),
            size: Some(Vector3 {
                x: 0.5,
                y: 0.5,
                z: 0.5,
            }),
            color: Some(Color {
                r: 0.5,
                g: 0.3,
                b: 1.0,
                a: 1.0,
            }),
        };
        let time_nanos = seconds_to_nanos(sphere.time);
        let entity = SceneEntity {
            timestamp: Some(get_timestamp(sphere.time)),
            frame_id: "scene".to_string(),
            id: sphere.id.clone(),
            lifetime: None,
            frame_locked: false,
            metadata: vec![],
            arrows: vec![],
            cubes: vec![],
            spheres: vec![sphere_primitive],
            cylinders: vec![],
            lines: vec![],
            triangles: vec![],
            texts: vec![],
            models: vec![],
        };
        Message {
            channel_id: SPHERE_CHANNEL_ID,
            log_time: time_nanos,
            publish_time: time_nanos,
            data: SceneUpdate {
                deletions: vec![],
                entities: vec![entity],
            }
            .encode_to_vec(),
        }
    }
}

impl From<&Temperature> for Message {
    fn from(temperature: &Temperature) -> Message {
        let time = seconds_to_nanos(temperature.time);
        Message {
            channel_id: TEMPERATURE_CHANNEL_ID,
            log_time: time,
            publish_time: time,
            data: serde_json::to_vec(temperature).expect("failed to stringify Temperature"),
        }
    }
}
