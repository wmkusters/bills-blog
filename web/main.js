import init from "./pkg/bills_blog.js";

const runWasm = async () => {
  // Instantiate our wasm module
    init().then(() => {console.log("WASM loaded")});
};
runWasm();
