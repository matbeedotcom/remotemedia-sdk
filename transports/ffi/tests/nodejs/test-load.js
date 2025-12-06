// Test native module loading
const m = require('../../nodejs/index.js');
console.log('Loaded:', m.isNativeLoaded());
if (!m.isNativeLoaded()) {
  console.log('Error:', m.getLoadError());
} else {
  console.log('Available exports:', Object.keys(m));
}
