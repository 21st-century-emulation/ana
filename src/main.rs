extern crate warp;
extern crate reqwest;

use std::collections::HashMap;
use serde::{ Deserialize, Serialize };
use std::env;
use warp::Filter;

#[derive(Deserialize, Serialize)]
struct CpuFlags {
    sign: bool,
    zero: bool,
    #[serde(rename = "auxCarry")] aux_carry: bool,
    parity: bool,
    carry: bool
}

#[derive(Deserialize, Serialize)]
struct CpuState {
    a: u8,
    b: u8,
    c: u8,
    d: u8,
    e: u8,
    h: u8,
    l: u8,
    #[serde(rename = "stackPointer")] stack_pointer: u16,
    #[serde(rename = "programCounter")] program_counter: u16,
    cycles: u64,
    flags: CpuFlags
}

#[derive(Deserialize, Serialize)]
struct Cpu {
    state: CpuState,
    opcode: u8
}

async fn execute(mut cpu: Cpu) -> Result<impl warp::Reply, warp::Rejection> {
    cpu.state.cycles += match cpu.opcode {
        0xA6 => 7,
        0xA0 | 0xA1 | 0xA2 | 0xA3 | 0xA4 | 0xA5 | 0xA7 => 4,
        _ => panic!("Invalid opcode passed to ANA")
    };

    let operand = match cpu.opcode {
        0xA0 => cpu.state.b,
        0xA1 => cpu.state.c,
        0xA2 => cpu.state.d,
        0xA3 => cpu.state.e,
        0xA4 => cpu.state.h,
        0xA5 => cpu.state.l,
        0xA6 => {
            let read_api = match env::var("READ_MEMORY_API") {
                Ok(url) => url,
                Err(_) => panic!("Couldn't read READ_MEMORY_API environment variable"),
            };
            let address = (u16::from(cpu.state.h) << 8) | u16::from(cpu.state.l);

            match reqwest::get(format!("{}?address={}", read_api, address)).await {
                Ok(result) => match result.text().await {
                    Ok(s) => s.as_str().parse::<u8>().unwrap(),
                    Err(e) => panic!("Invalid response from read memory API {}", e),
                },
                Err(err) => panic!("Invalid response from read memory API {}", err),
            }
        },
        0xA7 => cpu.state.a,
        _ => panic!("Invalid opcode passed to ANA")
    };

    let result = cpu.state.a & operand;
    cpu.state.flags.sign = (result & 0b1000_0000) == 0b1000_0000;
    cpu.state.flags.zero = result == 0;
    cpu.state.flags.parity = (result.count_ones() & 0b1) == 0;
    cpu.state.flags.carry = false;
    cpu.state.a = result;

    Ok(warp::reply::json(&cpu))
}

#[tokio::main]
async fn main() {
    let status = warp::get()
        .and(warp::path!("status"))
        .map(|| {
            "Healthy"
        });

    let read_memory = warp::get()
        .and(warp::path!("api"/"v1"/"debug"/"readMemory"))
        .and(warp::query::<HashMap<String, String>>())
        .map(|p: HashMap<String, String>| match p.get("address") {
            Some(_) => Ok(10.to_string()),
            None => panic!("Invalid request for memory in debug api"),
        });

    let execute = warp::post()
        .and(warp::path!("api"/"v1"/"execute"))
        .and(warp::body::content_length_limit(1024 * 16))
        .and(warp::body::json())
        .and_then(execute);

    warp::serve(execute.or(read_memory).or(status)).run(([0, 0, 0, 0], 8080)).await
}