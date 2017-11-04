#![feature(try_trait)]

extern crate pgn_reader;
extern crate memmap;
extern crate madvise;
extern crate btoi;
extern crate rand;

use std::env;
use std::str;
use std::fs::File;
use std::option::NoneError;

use memmap::Mmap;
use madvise::{AccessPattern, AdviseMemory};
use pgn_reader::{Visitor, Skip, Reader, San};
use btoi::ParseIntegerError;
use rand::{random, Closed01};

#[derive(Debug)]
enum TimeControl {
    UltraBullet,
    Bullet,
    Blitz,
    Classical,
    Correspondence,
}

#[derive(Debug)]
struct TimeControlError;

impl From<NoneError> for TimeControlError {
    fn from(_: NoneError) -> TimeControlError {
        TimeControlError { }
    }
}

impl From<ParseIntegerError> for TimeControlError {
    fn from(_: ParseIntegerError) -> TimeControlError {
        TimeControlError { }
    }
}

impl TimeControl {
    fn from_seconds_and_increment(seconds: u64, increment: u64) -> TimeControl {
        let total = seconds + 40 * increment;

        if total < 30 {
            TimeControl::UltraBullet
        } else if total < 180 {
            TimeControl::Bullet
        } else if total < 480 {
            TimeControl::Blitz
        } else if total < 21600 {
            TimeControl::Classical
        } else {
            TimeControl::Correspondence
        }
    }

    fn from_bytes(bytes: &[u8]) -> Result<TimeControl, TimeControlError> {
        if bytes == b"-" {
            return Ok(TimeControl::Correspondence);
        }

        let mut parts = bytes.splitn(2, |ch| *ch == b'+');
        let seconds = btoi::btou(parts.next()?)?;
        let increment = btoi::btou(parts.next()?)?;
        Ok(TimeControl::from_seconds_and_increment(seconds, increment))
    }
}

struct Indexer {
    white_elo: i16,
    black_elo: i16,
    time_control: TimeControl,
    skip: bool,
}

impl Indexer {
    fn new() -> Indexer {
        Indexer {
            white_elo: 0,
            black_elo: 0,
            time_control: TimeControl::Correspondence,
            skip: true,
        }
    }
}

impl<'pgn> Visitor<'pgn> for Indexer {
    type Result = ();

    fn begin_headers(&mut self) {
        self.white_elo = 0;
        self.black_elo = 0;
        self.time_control = TimeControl::Correspondence;
    }

    fn header(&mut self, key: &'pgn [u8], value: &'pgn [u8]) {
        if key == b"WhiteElo" {
            self.white_elo = if value == b"?" { 0 } else { btoi::btoi(value).expect("WhiteElo") };
        } else if key == b"BlackElo" {
            self.black_elo = if value == b"?" { 0 } else { btoi::btoi(value).expect("BlackElo") };
        } else if key == b"TimeControl" {
            self.time_control = TimeControl::from_bytes(value).expect("TimeControl");
        }
    }

    fn end_headers(&mut self) -> Skip {
        let rating = (self.white_elo + self.black_elo) / 2;

        let probability = match self.time_control {
            TimeControl::Correspondence => 1.0,
            TimeControl::Classical if rating >= 2000 => 1.0,
            TimeControl::Classical if rating >= 1800 => 2.0 / 5.0,
            TimeControl::Classical => 1.0 / 8.0,
            TimeControl::Blitz if rating >= 2000 => 1.0,
            TimeControl::Blitz if rating >= 1800 => 1.0 / 4.0,
            TimeControl::Blitz => 1.0 / 15.0,
            TimeControl::Bullet if rating >= 2300 => 1.0,
            TimeControl::Bullet if rating >= 2200 => 4.0 / 5.0,
            TimeControl::Bullet if rating >= 2000 => 1.0 / 4.0,
            TimeControl::Bullet if rating >= 1800 => 1.0 / 7.0,
            _ => 1.0 / 20.0,
        };

        let Closed01(rnd) = random::<Closed01<f64>>();
        self.skip = probability < rnd;
        Skip(self.skip)
    }

    fn san(&mut self, _san: San) { }

    fn begin_variation(&mut self) -> Skip {
        Skip(true) // stay in the mainline
    }

    fn end_game(&mut self, game: &'pgn [u8]) {
        println!("{}", str::from_utf8(game).expect("utf8"));
    }
}

fn main() {
    for arg in env::args().skip(1) {
        eprintln!("% indexing {} ...", arg);
        let file = File::open(&arg).expect("fopen");
        let pgn = unsafe { Mmap::map(&file).expect("mmap") };
        pgn.advise_memory_access(AccessPattern::Sequential).expect("madvise");

        let mut indexer = Indexer::new();
        Reader::new(&mut indexer, &pgn[..]).read_all();
    }
}
