let fs = require("fs");
let child_process = require("child_process");

let input = process.argv.slice(2);

if (__filename.includes("node_modules")) {
  fs.unlinkSync(__filename);
  fs.copySync("");
} else {
  child_process
    .spawn("target/release/pyn", input, { stdio: "inherit" })
    .on("exit", process.exit);
}
