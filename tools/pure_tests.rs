//! Raíz de crate para probar los módulos de lógica pura sin compilar el juego (tools/test.sh).
//! Todo módulo que no use `horizon_client` va aquí.
#![allow(dead_code)]

#[path = "../parkour/api/src/cinematic.rs"]
pub mod cinematic;
#[path = "../parkour/api/src/config.rs"]
pub mod config;
#[path = "../parkour/api/src/land.rs"]
pub mod land;
#[path = "../parkour/api/src/mantle.rs"]
pub mod mantle;
#[path = "../parkour/api/src/net.rs"]
pub mod net;
#[path = "../parkour/api/src/sense.rs"]
pub mod sense;
#[path = "../parkour/api/src/state.rs"]
pub mod state;
