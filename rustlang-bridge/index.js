const { spawn } = require("child_process");
const EventEmitter = require("events");
const { v4: uuidv4 } = require('uuid');

class RustBridge extends EventEmitter {
    /**
     * Creates a bridge between the given rust executable.
     * 
     * Node.js:
     * ```js
     * import RustBridge from "rustlang-bridge";
     * 
     * const bridge = new RustBridge("/path/to/rust_executable");
     * await new Promise(resolve => setTimeout(resolve, 1000)); // wait for the functions to initialize
     * console.log(await bridge.addition(10, 20)); // "30"
     * console.log(await bridge.find_longer("foo", "longer_foo")); // "longer_foo"
     * bridge.on("channel_foo", data => {
     *     console.log(data); // "bar"
     * });
     * bridge.send("channel_a", "Sent this from node!");
     * ```
     * Rust:
     * ```rust
     * use node_bridge::NodeBridge;
     *  
     * #[tokio::main]
     * async fn main() {
     *     let bridge = NodeBridge::new();
     *     bridge.register("addition", add, None);
     *     bridge.register_async("find_longer", find_longer, Some("Variable to pass to the function"));
     *     assert_eq!(bridge.receive("channel_a").await.unwrap(), "Sent this from node!");
     *     bridge.send("channel_foo", "bar").ok();
     *     bridge.wait_until_closes().await;
     * }
     * 
     * fn add(params: Vec<i32>, _: Option<()>) -> i32 {
     *     params[0] + params[1]
     * }
     * 
     * async fn find_longer(params: Vec<String>, pass_data: Option<&str>) -> String {
     *     assert_eq!(pass_data, Some("Variable to pass to the function"));
     *     if params[0].len() > params[1].len() {
     *         return params[0].to_string();
     *     }
     *     params[1].to_string()
     * }
     * ```
     * @param {string} executable_path Path to the built rust executable.
     */
    constructor(executable_path) {
        super();
        let rust_process = spawn(executable_path);
        this.rust_process = rust_process;
        this.is_closed = false;
        let response_waiting = [];
        rust_process.on("exit", () => {
            this.is_closed = true;
        });
        let handle_data = (data) => {
            if (data == "_bridge_exit") return rust_process.stdin.write("[bridgeexit]_\n");
            if (data.startsWith("fnregister")) {
                let name = data.split("_");
                name.shift();
                name = name.join("_");
                this[name] = (...params) => {
                    return new Promise((resolve) => {
                        let uid = uuidv4();
                        response_waiting.push({
                            uid, r: resolve
                        });
                        let first_param = params[0];
                        if (!first_param) first_param = "noarg";
                        else if (first_param && params.length == 1) {
                            if (typeof first_param !== "string") first_param = first_param.toString();
                            first_param += "[bridgeendline]";
                        }
                        rust_process.stdin.write(`function__bridge_name[${name}]_end_name_bridge_id[${uid}]_end_id_bridge_arg[${first_param}]_end_arg\n`);
                        if (params.length > 1) {
                            params.forEach((param, index) => {
                                if (index == 0) return;
                                if (typeof param !== "string") param = param.toString();
                                if (index == params.length - 1) param += "[bridgeendline]";
                                rust_process.stdin.write(`param_${uid}_${param}\n`);
                            });
                        }
                    });
                }
            } else if (data.startsWith("fnresponse")) {
                let splitted = data.split("_");
                splitted.shift();
                let id = splitted.shift();
                let dat = splitted.join("_");
                let r = response_waiting.find(x => x.uid == id);
                if (r) {
                    response_waiting = response_waiting.filter(x => x.uid !== id);
                    r.r(dat);
                }
            } else if (data.startsWith("tonode")) {
                let n_v = data.split("tonode__bridge_name[")[1].split("]_end_name");
                let name = n_v[0];
                let dat = n_v[1];
                this.emit(name, dat);
            }
        }
        rust_process.stdout.on("data", data => {
            data = data.toString().trim();
            let sp = data.split("[_bridgeendline]");
            sp.pop();
            sp.forEach(x => {
                if (x.startsWith("\n")) x = x.replace("\n", "");
                handle_data(x);
            });
        });
    }

    /**
     * 
     * @param {string} channel_name Name of the channel that we are sending to.
     * @param {string} data Data that we are sending.
     */
    send(channel_name, data) {
        this.rust_process.stdin.write(`torust__bridge_name[${channel_name}]_end_name${data}\n`);
    }
    /**
     * Closes the bridge.
     */
    close() {
        this.rust_process.stdin.write("[bridgeexit]_\n");
        return true;
    }

    /**
     * Returns whether the channel is closed or not.
     * @returns {boolean} 
     */
    is_closed() {
        return this.is_closed;
    }
}
module.exports = RustBridge;