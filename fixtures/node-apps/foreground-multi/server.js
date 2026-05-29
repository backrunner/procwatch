const http = require("node:http");

const port = Number(process.env.PORT || 3211);
const name = process.env.APP_NAME || "foreground";

http
  .createServer((_, res) => {
    res.end(name);
  })
  .listen(port, () => {
    console.log(`${name} listening on ${port}`);
  });
