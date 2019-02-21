extern crate hyper;
extern crate schedule_recv;
extern crate serde;
extern crate serde_json;
extern crate time;

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde_derive;

use hyper::rt::Future;
use hyper::service::service_fn_ok;
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use schedule_recv::periodic_ms;
use std::collections::HashMap;
use std::collections::HashSet;
use std::process::Command;

use std::sync::Mutex;
use std::thread;

lazy_static! {
    static ref USERS: Mutex<HashMap<usize, HashSet<usize>>> = Mutex::new(HashMap::new());
}

struct Process {
    uid: usize,
    pid: usize,
}

#[derive(Serialize)]
struct UserProcessCount {
    uid: usize,
    process_count: usize,
}

fn get_ps_output() -> Vec<Process> {
    let output = Command::new("ps")
        .arg("-o")
        .arg("ruid,pid")
        .arg("-ax")
        .output()
        .expect("failed to execute process");

    let stdout = String::from_utf8(output.stdout).unwrap();
    stdout
        .lines()
        .skip(1)
        .map(|x| {
            let a = x.split(' ').filter(|&x| x != "").collect::<Vec<&str>>();
            Process {
                uid: a[0].parse().unwrap_or(0),
                pid: a[1].parse().unwrap_or(0),
            }
        })
        .collect::<Vec<Process>>()
}

fn update_users() {
    let ps_output = get_ps_output();
    match USERS.lock() {
        Ok(mut users) => {
            for process in ps_output {
                match (*users).get_mut(&process.uid) {
                    Some(user_processes) => {
                        user_processes.insert(process.pid);
                    }
                    None => {
                        let mut user_processes = HashSet::new();
                        user_processes.insert(process.pid);
                        (*users).insert(process.uid, user_processes);
                    }
                }
            }
        }
        _ => (),
    }
}

fn show_users(req: Request<Body>) -> Response<Body> {
    let mut response = Response::new(Body::empty());

    match (req.method(), req.uri().path()) {
        (&Method::GET, "/") => {
            match USERS.lock() {
                Ok(users) => {
                    let mut user_processes =
                        (*users).iter().collect::<Vec<(&usize, &HashSet<usize>)>>();
                    user_processes.sort_by(|b, a| b.0.cmp(a.0));
                    match serde_json::to_string(
                        &user_processes
                            .iter()
                            .map(|(&key, processes)| UserProcessCount {
                                uid: key,
                                process_count: processes.len(),
                            })
                            .collect::<Vec<UserProcessCount>>(),
                    ) {
                        Ok(output) => *response.body_mut() = Body::from(output),
                        _ => *response.status_mut() = StatusCode::from_u16(500).unwrap(),
                    }
                }
                _ => *response.status_mut() = StatusCode::from_u16(500).unwrap(),
            };
        }

        _ => {
            *response.status_mut() = StatusCode::NOT_FOUND;
        }
    };

    response
}

fn update_user_processes() {
    let update_frequency = periodic_ms(1000);
    loop {
        update_frequency.recv().unwrap();
        update_users();
    }
}

fn main() {
    let addr = ([127, 0, 0, 1], 3000).into();

    let server = Server::bind(&addr)
        .serve(|| service_fn_ok(show_users))
        .map_err(|e| eprintln!("server error: {}", e));

    thread::spawn(update_user_processes);
    println!("Running Server");
    hyper::rt::run(server);
}
