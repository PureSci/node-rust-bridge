//! An easy bridge package to communicate between Node.js and Rust using std io.
//!
//! Created to use with the npm package `rustlang-bridge`
//!
//! # Examples
//!
//! Rust:
//! ```rust
//! use node_bridge::NodeBridge;
//!
//! #[tokio::main]
//! async fn main() {
//!     let bridge = NodeBridge::new();
//!     bridge.register("addition", add, None);
//!     bridge.register_async("find_longer", find_longer, Some("Variable to pass to the function"));
//!     assert_eq!(bridge.receive("channel_a").await.unwrap(), "Sent this from node!");
//!     bridge.send("channel_foo", "bar").ok();
//!     bridge.wait_until_closes().await;
//! }
//!
//! fn add(params: Vec<i32>, _: Option<()>) -> i32 {
//!     params[0] + params[1]
//! }
//!
//! async fn find_longer(params: Vec<String>, pass_data: Option<&str>) -> String {
//!     assert_eq!(pass_data, Some("Variable to pass to the function"));
//!     if params[0].len() > params[1].len() {
//!         return params[0].to_string();
//!     }
//!     params[1].to_string()
//! }
//! ```
//!
//! Node.js:
//! ```javascript
//! import RustBridge from "rustlang-bridge";
//!
//! const bridge = new RustBridge("/path/to/rust_executable");
//! await new Promise(resolve => setTimeout(resolve, 1000)); // wait for the functions to initialize
//! console.log(await bridge.addition(10,20)); // "30"
//! console.log(await bridge.find_longer("foo","longer_foo")); // "longer_foo"
//! bridge.on("channel_foo", data => {
//!     console.log(data); // "bar"
//! });
//! bridge.send("channel_a","Sent this from node!");
//! ```

use async_fn_traits::AsyncFn2;
use std::collections::HashMap;
use std::fmt::Debug;
use std::io;
use std::str::FromStr;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio::sync::oneshot::{channel as oneshot_channel, Sender as OneshotSender};

/// Occurs when you try to send or receive a message through the bridge after it has been closed.
///
/// # Examples
///
/// ```
/// use node_bridge::{NodeBridge, BridgeClosedError};
///
/// #[tokio::main]
/// async fn main() {
///     let bridge = NodeBridge::new();
///     bridge.close().await;
///     assert_eq!(bridge.receive("channel_foo").await, Err(BridgeClosedError));
///     assert_eq!(bridge.send("channel_foo", "bar"), Err(BridgeClosedError));
/// }
/// ```
#[derive(Debug)]
pub struct BridgeClosedError;

enum UtilType {
    ReadLine(String),
    Receive(String, OneshotSender<String>),
    RegisterFunction(String, UnboundedSender<Vec<String>>),
}

/// The Bridge between node and rust.
#[derive(Clone)]
pub struct NodeBridge {
    util_sender: UnboundedSender<UtilType>,
}

impl NodeBridge {
    /// Creates a bridge between node and rust. There can be only 1 bridge per rust program.
    pub fn new() -> NodeBridge {
        let (util_sender, mut util_receiver) = unbounded_channel();
        let u_s = util_sender.clone();
        tokio::spawn(async move {
            let mut receive_map: HashMap<String, OneshotSender<String>> = HashMap::new();
            let mut function_map: HashMap<String, UnboundedSender<Vec<String>>> = HashMap::new();
            let mut waiting_params: HashMap<String, (String, Vec<String>)> = HashMap::new();
            loop {
                match util_receiver.recv().await.unwrap() {
                    UtilType::ReadLine(data) => {
                        let splitted = data.split_once("_").unwrap();
                        match splitted.0 {
                            "torust" => {
                                let n_v = data
                                    .split_once("torust__bridge_name[")
                                    .unwrap()
                                    .1
                                    .split_once("]_end_name")
                                    .unwrap();
                                loop {
                                    match receive_map.remove(n_v.0) {
                                        Some(d) => {
                                            d.send(n_v.1.to_string()).ok();
                                        }
                                        None => {
                                            break;
                                        }
                                    }
                                }
                            }
                            "function" => {
                                let second_splitted = splitted.1.split_once("_").unwrap().1;
                                let name = second_splitted
                                    .split_once("bridge_name[")
                                    .unwrap()
                                    .1
                                    .split_once("]_end_name")
                                    .unwrap()
                                    .0;
                                let id = second_splitted
                                    .split_once("_bridge_id[")
                                    .unwrap()
                                    .1
                                    .split_once("]_end_id")
                                    .unwrap()
                                    .0;
                                let arg = second_splitted
                                    .split_once("_bridge_arg[")
                                    .unwrap()
                                    .1
                                    .split_once("]_end_arg")
                                    .unwrap()
                                    .0;
                                if arg == "noarg" {
                                    match function_map.get(name) {
                                        Some(d) => {
                                            d.send(vec![id.to_string()]).ok();
                                        }
                                        None => {}
                                    }
                                } else {
                                    if arg.ends_with("[bridgeendline]") {
                                        match function_map.get(name) {
                                            Some(f) => {
                                                f.send(vec![
                                                    id.to_string(),
                                                    arg.replace("[bridgeendline]", "").to_string(),
                                                ])
                                                .ok();
                                            }
                                            None => {}
                                        }
                                    } else {
                                        waiting_params.insert(
                                            id.to_string(),
                                            (
                                                name.to_string(),
                                                vec![id.to_string(), arg.to_string()],
                                            ),
                                        );
                                    }
                                }
                            }
                            "param" => {
                                let (id, value) = splitted.1.split_once("_").unwrap();
                                match waiting_params.get_mut(&id.to_string()) {
                                    Some(d) => {
                                        if value.ends_with("[bridgeendline]") {
                                            d.1.push(
                                                value.replace("[bridgeendline]", "").to_string(),
                                            );
                                            match function_map.get(&d.0) {
                                                Some(f) => {
                                                    f.send(d.1.to_owned()).ok();
                                                }
                                                None => {}
                                            }
                                            waiting_params.remove(&id.to_string());
                                        } else {
                                            d.1.push(value.to_string());
                                        }
                                    }
                                    None => {}
                                }
                            }
                            "[bridgeexit]" => {
                                util_receiver.close();
                                break;
                            }
                            _ => {}
                        }
                    }
                    UtilType::Receive(name, return_sender) => {
                        receive_map.insert(name, return_sender);
                    }
                    UtilType::RegisterFunction(name, sender) => {
                        function_map.insert(name, sender);
                    }
                }
            }
        });
        tokio::task::spawn_blocking(move || loop {
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            let trimmed = input.trim().to_string();
            if trimmed == "[bridgeexit]_" {
                util_sender.send(UtilType::ReadLine(trimmed)).ok();
                break;
            }
            util_sender.send(UtilType::ReadLine(trimmed)).ok();
        });
        NodeBridge { util_sender: u_s }
    }

    /// Sends a message through a channel to node.
    /// Results with an Err(BridgeClosedError) if the bridge is already closed.
    /// ```
    /// assert_eq!(bridge.send("channel_foo", "bar"), Ok(()));
    /// bridge.close().await;
    /// assert_eq!(bridge.send("channel_foo", "bar"), Err(BridgeClosedError));
    /// ```
    pub fn send<T: ToString, F: ToString>(
        &self,
        channel_name: T,
        data: F,
    ) -> Result<(), BridgeClosedError> {
        if self.util_sender.is_closed() {
            return Err(BridgeClosedError);
        }
        println!(
            "tonode__bridge_name[{}]_end_name{}[_bridgeendline]",
            channel_name.to_string(),
            data.to_string()
        );
        Ok(())
    }

    /// Waits until the bridge is closed in some way to prevent it from closing on its own.
    pub async fn wait_until_closes(&self) {
        self.util_sender.closed().await;
    }

    /// Returns whether the bridge is closed or not.
    pub fn is_closed(&self) -> bool {
        if self.util_sender.is_closed() {
            return true;
        }
        false
    }

    /// Closes the bridge and waits until its close.
    pub async fn close(&self) {
        println!("_bridge_exit[_bridgeendline]");
        self.util_sender.closed().await;
    }

    /// Receives a message through a channel from node.
    /// Results with an Err(BridgeClosedError) if the bridge is already closed.
    /// ```
    /// bridge.receive("channel_foo").await.unwrap();
    /// bridge.close().await;
    /// assert_eq!(bridge.receive("channel_foo").await, Err(BridgeClosedError));
    /// ```
    pub async fn receive<T: ToString>(&self, channel_name: T) -> Result<String, BridgeClosedError> {
        let (sender, receiver) = oneshot_channel();
        let _ = &self
            .util_sender
            .send(UtilType::Receive(channel_name.to_string(), sender));
        match receiver.await {
            Ok(d) => Ok(d),
            Err(_) => Err(BridgeClosedError),
        }
    }

    /// Registers a sync function that can be called from node as long as the bridge is not closed.
    pub fn register<T: ToString + 'static, D: Send + 'static + Clone, B: FromStr + 'static>(
        &self,
        name: &str,
        function: fn(Vec<B>, Option<D>) -> T,
        pass_data: Option<D>,
    ) where
        <B as FromStr>::Err: Debug,
    {
        println!("fnregister_{}[_bridgeendline]", name);
        let (sender, mut receiver) = unbounded_channel();
        let _ = &self
            .util_sender
            .send(UtilType::RegisterFunction(name.to_string(), sender));
        tokio::spawn(async move {
            loop {
                match receiver.recv().await {
                    Some(mut params) => {
                        let id = params.remove(0);
                        let o = params.iter().map(|x| x.parse::<B>().unwrap()).collect();
                        println!(
                            "fnresponse_{}_{}[_bridgeendline]",
                            id,
                            function(o, pass_data.clone()).to_string()
                        );
                    }
                    None => {
                        break;
                    }
                }
            }
        });
    }

    /// Registers an async function that can be called from node as long as the bridge is not closed.
    pub fn register_async<T: ToString, F, D>(&self, name: &str, function: F, pass_data: Option<D>)
    where
        F: AsyncFn2<Vec<String>, Option<D>, Output = T> + Send + Sync + 'static,
        <F as AsyncFn2<Vec<String>, Option<D>>>::OutputFuture: Send,
        D: Send + Clone + Sync + 'static,
    {
        println!("fnregister_{}[_bridgeendline]", name);
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let _ = &self
            .util_sender
            .send(UtilType::RegisterFunction(name.to_string(), sender));
        tokio::spawn(async move {
            loop {
                match receiver.recv().await {
                    Some(mut params) => {
                        let id = params.remove(0);
                        println!(
                            "fnresponse_{}_{}[_bridgeendline]",
                            id,
                            function(params, pass_data.clone()).await.to_string()
                        );
                    }
                    None => {
                        break;
                    }
                }
            }
        });
    }
}
