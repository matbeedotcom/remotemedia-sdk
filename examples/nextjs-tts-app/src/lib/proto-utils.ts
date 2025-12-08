/**
 * Re-export proto utilities from the RemoteMedia FFI package
 *
 * These utilities work in both browser and Node.js environments
 * for encoding/decoding DataBuffer protobuf messages.
 */

// Note: For production, you would import from '@remotemedia/nodejs-ffi/proto-utils'
// For development, we use a relative path to the source
// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-ignore - path is resolved at build time
export {
  encodeTextData,
  encodeJsonData,
  decodeDataBuffer,
  decodeTextBuffer,
  decodeJsonData,
  decodeAudioBuffer,
  parseJsonFromDataBuffer,
  type DecodedTextBuffer,
  type DecodedJsonBuffer,
  type DecodedAudioBuffer,
  type DecodedDataBuffer,
} from '../../../../transports/ffi/nodejs/proto-utils';
