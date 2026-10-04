// The audited build command inherits this check in its Node children.
const fs = require('node:fs');
const crypto = require('node:crypto');
const checksum = (file) => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const expected = JSON.parse(process.env.ARCHGUARD_BUILD_NODE);
const path = fs.realpathSync(process.execPath);
const runtime = { path, version: process.version, sha256: checksum(path) };
const entryPath = process.argv[1] ? fs.realpathSync(process.argv[1]) : null;
const record = { pid: process.pid, parentPid: process.ppid, runtime, entryPath,
  entrySha256: entryPath ? checksum(entryPath) : null };
const destination = process.env.ARCHGUARD_BUILD_RUNTIME;
const line = JSON.stringify(record) + '\n';
const size = fs.existsSync(destination) ? fs.statSync(destination).size : 0;
if (size + Buffer.byteLength(line) > 256 * 1024) throw new Error('Build runtime evidence exceeds byte budget');
fs.appendFileSync(destination, line);
if (runtime.path !== expected.path || runtime.version !== expected.version || runtime.sha256 !== expected.sha256) {
  throw new Error('Actual build Node runtime disagrees with its frozen identity');
}
