# Node Bridge

Simple rust script to achieve a bridge between node.js and rust. Use with the npm package rustlang-bridge.
Only 1 bridge can be initialized per rust program. But node.js can have many bridges.

## Installation

Add the following line to your Cargo.toml under [dependencies]

```toml
node_bridge = "1.0.0"
```
    
Node.js installation:
```bash
$ npm install rustlang-bridge
```
## Usage/Examples

```rust
use node_bridge::NodeBridge;

#[tokio::main]
async fn main() {
    let bridge = NodeBridge::new();
    bridge.register("addition", add, None);
    bridge.register_async("find_longer", find_longer, Some("Variable to pass to the function"));
    assert_eq!(bridge.receive("channel_a").await.unwrap(), "Sent this from node!");
    bridge.send("channel_foo", "bar").ok();
    bridge.wait_until_closes().await;
}
 
fn add(params: Vec<i32>, _: Option<()>) -> i32 {
    params[0] + params[1]
}
 
async fn find_longer(params: Vec<String>, pass_data: Option<&str>) -> String {
    assert_eq!(pass_data, Some("Variable to pass to the function"));
    if params[0].len() > params[1].len() {
        return params[0].to_string();
    }
    params[1].to_string()
}
```

Handling in node.js:

```javascript
import RustBridge from "rustlang-bridge";
 
const bridge = new RustBridge("/path/to/rust_executable");
await new Promise(resolve => setTimeout(resolve, 500)); // wait for the functions to initialize
console.log(await bridge.addition(10, 20)); // "30"
console.log(await bridge.find_longer("foo", "longer_foo")); // "longer_foo"
bridge.on("channel_foo", data => {
    console.log(data); // "bar"
});
bridge.send("channel_a", "Sent this from node!");
```
## Contributing

Contributions are always welcome!

