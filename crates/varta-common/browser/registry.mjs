// Generated from varta-common registries; do not edit.
// Regenerate: cargo run -p varta-common --example generate_browser_registry --locked
function freeze(value) {
  if (value !== null && typeof value === "object") {
    for (const child of Object.values(value)) freeze(child);
    Object.freeze(value);
  }
  return value;
}
export const registry = freeze({
  "booleanCapabilities": [
    "clipboard.write",
    "process.exit",
    "time.now",
    "time.timer"
  ],
  "console": {
    "logPrefix": "varta log: ",
    "stderr": [
      {
        "method": "error",
        "prefix": "varta error: ",
        "word": 0
      },
      {
        "method": "warn",
        "prefix": "varta warn: ",
        "word": 1
      }
    ]
  },
  "effects": {
    "clock": {
      "capability": "time.now",
      "denialText": "<capability denied: time.now> (Date.now)",
      "operation": "date_now"
    },
    "collector": {
      "deadlineBudget": "maintenanceDeadlineMs",
      "deadlineExhausted": "turn {turn} (maintenance) exhausted its maintenance deadline budget ({limit} ms per idle period); the instance was terminated",
      "done": 2,
      "fuelBudget": "maintenanceFuel",
      "fuelExhausted": "turn {turn} (maintenance) exhausted its maintenance fuel budget ({limit} instructions per idle period); the instance was terminated",
      "idleExport": "varta.collector.idle",
      "invalidStatus": "invalid idle collection status",
      "limitPlaceholder": "{limit}",
      "more": 1,
      "none": 0,
      "reportedPeriods": 256,
      "stepFuel": 1000000,
      "stepUnits": 2048,
      "turnPlaceholder": "{turn}"
    },
    "environment": {
      "capability": "env.get",
      "operation": "env_snapshot"
    },
    "exit": {
      "capability": "process.exit",
      "denialText": "<capability denied: process.exit> (process)",
      "maximum": 2147483647,
      "minimum": -2147483648,
      "operation": "process_exit_f64"
    },
    "failures": {
      "invalid": "<invalid host input>",
      "tooLarge": "<result too large>"
    },
    "fatal": {
      "fuelExhausted": "turn {turn} ({kind}) exhausted its fuel budget ({limit} instructions per turn); the instance was terminated",
      "kindPlaceholder": "{kind}",
      "kinds": {
        "event": "event",
        "main": "main",
        "network": "network",
        "timer": "timer",
        "view": "view"
      },
      "limitPlaceholder": "{limit}",
      "requestExport": "resource_request",
      "requestPlaceholder": "{requested}",
      "resourceExport": "resource_failure",
      "resources": {
        "1": "resource limit: memory budget exhausted (requested {requested})",
        "2": "resource limit: value exceeds the Wasm size limit (requested {requested})",
        "3": "resource limit: root stack exhausted (requested {requested})",
        "4": "resource limit: live continuation frames exhausted (requested {requested})"
      },
      "turnPlaceholder": "{turn}"
    },
    "localTime": {
      "defaultLocale": "en-US",
      "defaultTimeZone": "UTC",
      "locale": "intl_locale",
      "maxBytes": 256,
      "timeZone": "intl_time_zone"
    },
    "random": {
      "operation": "number_random",
      "sampleBits": 53,
      "wordBits": 64
    }
  },
  "environment": {
    "maxInputBytes": 65536,
    "maxMatchWork": 65536,
    "maxNameBytes": 256,
    "maxPatternBytes": 512,
    "maxPatterns": 64,
    "maxResultBytes": 1048576,
    "resultLength": "env_result_len",
    "resultStatus": "result_status",
    "statuses": {
      "denied": 1,
      "invalid": 2,
      "io": 3,
      "ok": 0,
      "tooLarge": 4
    }
  },
  "fuel": {
    "byte": 1,
    "call": 50,
    "jsonParseUnit": 30,
    "jsonParseValue": 1500,
    "jsonSchemaField": 3000,
    "jsonSchemaType": 3000,
    "jsonStringifyUnit": 30,
    "jsonStringifyValue": 5500,
    "kvByte": 10,
    "kvEntry": 2000,
    "kvOperation": 100000
  },
  "kv": {
    "capabilities": {
      "read": "kv.read",
      "write": "kv.write"
    },
    "capabilityPlaceholder": "{capability}",
    "checkFailed": 0.0,
    "codec": {
      "absent": 0,
      "entries": {
        "key": 1,
        "none": 0,
        "value": 2
      },
      "exports": {
        "configure": "kv_configure",
        "decode": "kv_decode",
        "input": "kv_input",
        "memory": "memory",
        "output": "kv_output",
        "outputLength": "kv_output_len",
        "parseCommit": "kv_parse_commit",
        "parseRead": "kv_parse_read",
        "readEntries": "kv_read_entries",
        "stage": "kv_stage"
      },
      "importModule": "varta_kv_host",
      "imports": {
        "allocate": "allocate",
        "channelReady": "channel_ready",
        "channelThrow": "channel_throw",
        "charge": "charge",
        "deadline": "deadline_passed",
        "memoryRead": "memory_read",
        "memorySize": "memory_size",
        "memoryWrite": "memory_write",
        "memoryZero": "memory_zero",
        "resultLength": "result_length",
        "retain": "retain"
      },
      "present": 1,
      "sections": {
        "intrinsics": "varta.intrinsics",
        "static": "varta.static"
      },
      "statuses": {
        "deadline": 3,
        "failed": 1,
        "fatal": 2,
        "ok": 0
      }
    },
    "commitReserveMs": 50,
    "committed": 1.0,
    "containerFormat": 1,
    "deadlineError": "browser execution deadline exceeded",
    "denialText": "<capability denied: {capability}> ({path})",
    "descriptorVersion": "varta-kv-1",
    "encodings": {
      "bytes": 3,
      "u64": 2,
      "v8": 1
    },
    "entryFields": {
      "bytes": 3,
      "cursor": 4,
      "encoding": 1,
      "range": 5,
      "u64": 2,
      "versionstamp": 0
    },
    "errors": {
      "notAllowed": "NotAllowedError",
      "notFound": "NotFoundError",
      "rangeError": "RangeError",
      "typeError": "TypeError"
    },
    "failed": -1.0,
    "failurePlaceholder": "{failure}",
    "fuel": {
      "operation": 100000
    },
    "guest": {
      "allocate": "varta.value_abi.allocate",
      "pending": "varta.exception.pending",
      "resultLength": "env_result_len",
      "retain": "varta.value_abi.retain"
    },
    "lockedErrors": [
      "NoModificationAllowedError",
      "InvalidStateError"
    ],
    "maxDescriptorBytes": 65536,
    "maxOpenDatabases": 8,
    "maxStaged": 2100,
    "maxStagedBytes": 901120,
    "maxValueDepth": 64,
    "mutationNames": {
      "2": "sum",
      "3": "min",
      "4": "max"
    },
    "mutations": {
      "max": 4,
      "min": 3,
      "remove": 1,
      "set": 0,
      "sum": 2
    },
    "noDirectories": "the browser host keeps KV databases only directly in a file root, with no directories",
    "noStorage": "no KV storage was acquired for this application",
    "notAllowed": "NotAllowedError",
    "operationPlaceholder": "{operation}",
    "operations": {
      "close": "kv_close",
      "commit": "kv_commit",
      "entry": "kv_entry",
      "key": "kv_key",
      "open": "kv_open",
      "read": "kv_read",
      "stage": "kv_stage",
      "text": "kv_text",
      "value": "kv_value"
    },
    "pathPlaceholder": "{path}",
    "paths": {
      "maxMatchWork": 65536,
      "maxPathBytes": 4096,
      "maxPatternBytes": 512,
      "maxPatterns": 64,
      "maxRootNameBytes": 64,
      "separator": "/"
    },
    "quotaError": "QuotaExceededError",
    "reservedNamePrefix": ":",
    "separator": "\n",
    "sidecarSuffixes": [
      "-wal",
      "-shm",
      "-journal"
    ],
    "stagedTooLarge": "the staged keys and values of one KV operation are too large",
    "storageUnavailable": "browser KV storage needs OPFS sync access handles in a dedicated Worker, which this host does not provide; the manifest grants ",
    "texts": {
      "corrupt": "{path} is not in a Varta KV container of format version 1: {failure}",
      "full": "the KV storage of {path} is full ({failure}); nothing was applied",
      "invalidName": "{path} is not a valid database file name",
      "locked": "the KV database {path} is locked by another writer",
      "noFile": "{path} names no database file",
      "nonU64": "Failed to perform '{operation}' mutation on a non-U64 value in the database",
      "notFound": "no KV database at {path}",
      "readOnly": "the KV database {path} is open read-only",
      "tooLate": "too little time is left in this turn to commit to the KV database {path}; nothing was applied",
      "tooManyDatabases": "at most 8 KV databases are open at once",
      "unavailable": "cannot open the KV database {path}: {failure}",
      "uncertain": "the KV storage of {path} is unavailable after an unfinished write; reload the application"
    },
    "valueExtent": {
      "maxUnits": 98304,
      "maxValues": 65536,
      "tooExpanded": "a KV value cannot expand to more than 65536 values or 98304 code units of text"
    },
    "valueTooDeep": "a KV value cannot nest deeper than 64 levels"
  },
  "maxTurnFuel": 1000000000,
  "meter": {
    "counter": "fuel",
    "module": "varta_browser_meter",
    "section": "varta.browser.meter",
    "version": 1
  },
  "network": {
    "byteBody": "2",
    "capability": "net.request",
    "completion": {
      "aborted": 5,
      "chunk": 2,
      "denied": 6,
      "end": 3,
      "failed": 4,
      "head": 1
    },
    "errors": {
      "bodyLimit": "response body exceeds 524288 bytes",
      "bodyNotAllowed": "a GET or HEAD request cannot have a body",
      "bodyTimeout": "response body timed out",
      "budget": "execution exceeds 8 pending network requests",
      "descriptor": "malformed network request descriptor",
      "headerBytes": "request headers are too large",
      "headerName": "invalid header name: {value}",
      "headerValue": "invalid header value for {value}",
      "method": "invalid method: {value}",
      "runtimeHeader": "header {value} is set by the runtime",
      "state": "invalid network request operation",
      "timeout": "request timed out",
      "url": "Invalid URL: {value}"
    },
    "forbiddenMethods": [
      "CONNECT",
      "TRACE",
      "TRACK"
    ],
    "forbiddenResponseHeaders": [
      "set-cookie",
      "set-cookie2"
    ],
    "headFields": [
      "status",
      "statusText",
      "url",
      "location",
      "headers"
    ],
    "maxBodyBytes": 524288,
    "maxChunkBytes": 65536,
    "maxHeaderNameBytes": 256,
    "maxMatchWork": 65536,
    "maxMethods": 16,
    "maxPatternBytes": 512,
    "maxPatterns": 64,
    "maxPending": 8,
    "maxReadBytes": 8192,
    "maxRequestBytes": 262144,
    "maxRequestHeaderBytes": 16384,
    "maxRequestHeaders": 64,
    "maxResponseHeaderBytes": 65536,
    "maxResponseHeaders": 100,
    "maxUrlBytes": 8192,
    "maxVisibleHeaders": 64,
    "noBody": "0",
    "normalizedMethods": [
      "DELETE",
      "GET",
      "HEAD",
      "OPTIONS",
      "POST",
      "PUT"
    ],
    "readInactivityMs": 300000,
    "replay": {
      "errors": {
        "abort": "network replay diverged: request {request} was aborted where the log continues",
        "descriptor": "network recording and replay serve net_request_start_v2 requests only",
        "read": "network replay diverged: request {request} read {actual}, the log recorded {logged}",
        "request": "network replay diverged: request {request} is {method} {url}, the log recorded {loggedMethod} {loggedUrl}",
        "stalled": "network replay stalled at log event {event}: no logged completion can reach an armed request",
        "unconsumed": "network replay ended with {remaining} logged events undelivered",
        "ungranted": "network replay refused: logged request {request} ({method} {url}) is not granted",
        "unlogged": "network replay diverged: request {method} {url} is not in the log"
      },
      "format": "varta-network-log",
      "kinds": [
        "head",
        "chunk",
        "end",
        "failed",
        "abort"
      ],
      "maxEvents": 65536,
      "maxFailureBytes": 8192,
      "maxIndex": 4294967295,
      "maxLogBytes": 16777216,
      "maxRequests": 4096,
      "maxStatus": 999,
      "minStatus": 100,
      "reads": [
        "text",
        "bytes"
      ],
      "unscriptedStep": 0,
      "version": 1
    },
    "result": {
      "base": 65536,
      "length": "env_result_len",
      "maxBytes": 1048576,
      "pageBytes": 65536
    },
    "runtimeHeaders": [
      "connection",
      "content-length",
      "expect",
      "host",
      "keep-alive",
      "te",
      "trailer",
      "transfer-encoding",
      "upgrade"
    ],
    "safelistedHeaders": [
      "cache-control",
      "content-language",
      "content-length",
      "content-type",
      "expires",
      "last-modified",
      "pragma"
    ],
    "separator": "\n",
    "textBody": "1",
    "version": "varta-net-request-2"
  },
  "operations": [
    {
      "admittedUnder": [
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 27,
      "name": "bigint_binary",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i64",
        "i64"
      ],
      "resultGlobals": true,
      "results": [
        "i32",
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 27,
      "name": "bigint_compare",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 27,
      "name": "bigint_compare_number",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "f64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 27,
      "name": "bigint_compare_string",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i32",
        "i32"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 27,
      "name": "bigint_from_number",
      "networkBookkeeping": false,
      "params": [
        "f64"
      ],
      "resultGlobals": true,
      "results": [
        "i32",
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 27,
      "name": "bigint_from_string",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i32",
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 28,
      "name": "bigint_from_value",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": true,
      "results": [
        "i32",
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v34"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 30,
      "name": "bigint_from_value_v2",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": true,
      "results": [
        "i32",
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v35"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 31,
      "name": "bigint_from_value_v3",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": true,
      "results": [
        "i32",
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 32,
      "name": "bigint_from_value_v4",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": true,
      "results": [
        "i32",
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "bigint_from_value_v5",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": true,
      "results": [
        "i32",
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 27,
      "name": "bigint_to_number",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 27,
      "name": "bigint_to_string",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "f64"
      ],
      "resultGlobals": true,
      "results": [
        "i32",
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 28,
      "name": "bigint_to_string_v2",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i64"
      ],
      "resultGlobals": true,
      "results": [
        "i32",
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 27,
      "name": "bigint_unary",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i64"
      ],
      "resultGlobals": true,
      "results": [
        "i32",
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 27,
      "name": "bigint_width",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i64",
        "f64"
      ],
      "resultGlobals": true,
      "results": [
        "i32",
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 28,
      "name": "bigint_width_v2",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i64",
        "i64"
      ],
      "resultGlobals": true,
      "results": [
        "i32",
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "console_render",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i64"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v37",
        "checked-f64-utf16-v38"
      ],
      "capability": "time.now",
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 33,
      "name": "date_now",
      "networkBookkeeping": false,
      "params": [],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "date_zone_name",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32",
        "i32",
        "f64"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "date_zone_offset",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "f64",
        "f64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-i32-utf8-v1",
        "checked-f64-utf8-v1",
        "checked-f64-utf8-v2"
      ],
      "capability": "env.get",
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 1,
      "name": "env_get",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v1",
        "checked-f64-utf16-v2",
        "checked-f64-utf16-v3",
        "checked-f64-utf16-v4",
        "checked-f64-utf16-v5",
        "checked-f64-utf16-v6",
        "checked-f64-utf16-v7",
        "checked-f64-utf16-v8",
        "checked-f64-utf16-v9",
        "checked-f64-utf16-v10",
        "checked-f64-utf16-v11",
        "checked-f64-utf16-v12",
        "checked-f64-utf16-v13",
        "checked-f64-utf16-v14",
        "checked-f64-utf16-v15",
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37",
        "checked-f64-utf16-v38"
      ],
      "capability": "env.get",
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 4,
      "name": "env_get16",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-i32-utf8-v1",
        "checked-f64-utf8-v1",
        "checked-f64-utf8-v2"
      ],
      "capability": "env.has",
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 1,
      "name": "env_has",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v1",
        "checked-f64-utf16-v2",
        "checked-f64-utf16-v3",
        "checked-f64-utf16-v4",
        "checked-f64-utf16-v5",
        "checked-f64-utf16-v6",
        "checked-f64-utf16-v7",
        "checked-f64-utf16-v8",
        "checked-f64-utf16-v9",
        "checked-f64-utf16-v10",
        "checked-f64-utf16-v11",
        "checked-f64-utf16-v12",
        "checked-f64-utf16-v13",
        "checked-f64-utf16-v14",
        "checked-f64-utf16-v15",
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37",
        "checked-f64-utf16-v38"
      ],
      "capability": "env.has",
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 4,
      "name": "env_has16",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-i32-utf8-v1",
        "checked-f64-utf8-v1",
        "checked-f64-utf8-v2"
      ],
      "capability": "env.print",
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 1,
      "name": "env_print",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v1",
        "checked-f64-utf16-v2",
        "checked-f64-utf16-v3",
        "checked-f64-utf16-v4",
        "checked-f64-utf16-v5",
        "checked-f64-utf16-v6",
        "checked-f64-utf16-v7",
        "checked-f64-utf16-v8",
        "checked-f64-utf16-v9",
        "checked-f64-utf16-v10",
        "checked-f64-utf16-v11",
        "checked-f64-utf16-v12",
        "checked-f64-utf16-v13",
        "checked-f64-utf16-v14",
        "checked-f64-utf16-v15",
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37",
        "checked-f64-utf16-v38"
      ],
      "capability": "env.print",
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 4,
      "name": "env_print16",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v38"
      ],
      "capability": "env.get",
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "env_snapshot",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": true,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "event_source_close",
      "networkBookkeeping": false,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": true,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "event_source_open",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": true,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "event_take",
      "networkBookkeeping": false,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": true,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "event_text",
      "networkBookkeeping": false,
      "params": [
        "f64"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": true,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "event_watch",
      "networkBookkeeping": false,
      "params": [
        "f64",
        "f64"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-i32-utf8-v1",
        "checked-f64-utf8-v1",
        "checked-f64-utf8-v2"
      ],
      "capability": "fs.print",
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 1,
      "name": "fs_print",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v1",
        "checked-f64-utf16-v2",
        "checked-f64-utf16-v3",
        "checked-f64-utf16-v4",
        "checked-f64-utf16-v5",
        "checked-f64-utf16-v6",
        "checked-f64-utf16-v7",
        "checked-f64-utf16-v8",
        "checked-f64-utf16-v9",
        "checked-f64-utf16-v10",
        "checked-f64-utf16-v11",
        "checked-f64-utf16-v12",
        "checked-f64-utf16-v13",
        "checked-f64-utf16-v14",
        "checked-f64-utf16-v15",
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37",
        "checked-f64-utf16-v38"
      ],
      "capability": "fs.print",
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 4,
      "name": "fs_print16",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-i32-utf8-v1",
        "checked-f64-utf8-v1",
        "checked-f64-utf8-v2"
      ],
      "capability": "fs.read",
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 1,
      "name": "fs_read",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v1",
        "checked-f64-utf16-v2",
        "checked-f64-utf16-v3",
        "checked-f64-utf16-v4",
        "checked-f64-utf16-v5",
        "checked-f64-utf16-v6",
        "checked-f64-utf16-v7",
        "checked-f64-utf16-v8",
        "checked-f64-utf16-v9",
        "checked-f64-utf16-v10",
        "checked-f64-utf16-v11",
        "checked-f64-utf16-v12",
        "checked-f64-utf16-v13",
        "checked-f64-utf16-v14",
        "checked-f64-utf16-v15",
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37",
        "checked-f64-utf16-v38"
      ],
      "capability": "fs.read",
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 4,
      "name": "fs_read16",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-i32-utf8-v1",
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 1,
      "name": "http_service",
      "networkBookkeeping": false,
      "params": [],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-i32-utf8-v1",
        "checked-f64-utf8-v1",
        "checked-f64-utf8-v2",
        "checked-f64-utf16-v1",
        "checked-f64-utf16-v2",
        "checked-f64-utf16-v3",
        "checked-f64-utf16-v4",
        "checked-f64-utf16-v5",
        "checked-f64-utf16-v6",
        "checked-f64-utf16-v7",
        "checked-f64-utf16-v8",
        "checked-f64-utf16-v9",
        "checked-f64-utf16-v10",
        "checked-f64-utf16-v11",
        "checked-f64-utf16-v12",
        "checked-f64-utf16-v13",
        "checked-f64-utf16-v14",
        "checked-f64-utf16-v15",
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37",
        "checked-f64-utf16-v38"
      ],
      "capability": "net.listen",
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 1,
      "name": "http_stop",
      "networkBookkeeping": false,
      "params": [],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "intl_locale",
      "networkBookkeeping": false,
      "params": [],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "intl_time_zone",
      "networkBookkeeping": false,
      "params": [],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-i32-utf8-v1"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 1,
      "name": "json_parse",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v6",
        "checked-f64-utf16-v7"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 9,
      "name": "json_parse_v10",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v8"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 11,
      "name": "json_parse_v11",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v9"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 12,
      "name": "json_parse_v12",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v10",
        "checked-f64-utf16-v11",
        "checked-f64-utf16-v12"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 13,
      "name": "json_parse_v13",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v13",
        "checked-f64-utf16-v14",
        "checked-f64-utf16-v15",
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 16,
      "name": "json_parse_v14",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 28,
      "name": "json_parse_v15",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v34"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 30,
      "name": "json_parse_v16",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v35"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 31,
      "name": "json_parse_v17",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 32,
      "name": "json_parse_v18",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "json_parse_v19",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf8-v1"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 2,
      "name": "json_parse_v3",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf8-v2"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 3,
      "name": "json_parse_v4",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v1"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 4,
      "name": "json_parse_v5",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v2"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 5,
      "name": "json_parse_v6",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v3"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 6,
      "name": "json_parse_v7",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v4"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 7,
      "name": "json_parse_v8",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v5"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 8,
      "name": "json_parse_v9",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-i32-utf8-v1"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 1,
      "name": "json_stringify",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "json_stringify_owned",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": false,
      "results": [
        "i64",
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v8"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 11,
      "name": "json_stringify_v10",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v9",
        "checked-f64-utf16-v10",
        "checked-f64-utf16-v11",
        "checked-f64-utf16-v12"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 12,
      "name": "json_stringify_v11",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v13",
        "checked-f64-utf16-v14",
        "checked-f64-utf16-v15",
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 16,
      "name": "json_stringify_v12",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 28,
      "name": "json_stringify_v13",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i32",
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v34"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 30,
      "name": "json_stringify_v14",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i32",
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v35"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 31,
      "name": "json_stringify_v15",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i32",
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v36"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 32,
      "name": "json_stringify_v16",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i32",
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 33,
      "name": "json_stringify_v17",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": false,
      "results": [
        "i32",
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf8-v1"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 2,
      "name": "json_stringify_v3",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf8-v2"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 3,
      "name": "json_stringify_v4",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v1"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 4,
      "name": "json_stringify_v5",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v2"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 5,
      "name": "json_stringify_v6",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v3"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 6,
      "name": "json_stringify_v7",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v4",
        "checked-f64-utf16-v5"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 7,
      "name": "json_stringify_v8",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v6",
        "checked-f64-utf16-v7"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 9,
      "name": "json_stringify_v9",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": true,
      "minHostAbi": 34,
      "name": "kv_close",
      "networkBookkeeping": false,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v38"
      ],
      "capability": "kv.write",
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "kv_commit",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": true,
      "minHostAbi": 34,
      "name": "kv_entry",
      "networkBookkeeping": false,
      "params": [
        "f64",
        "f64"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "kv_key",
      "networkBookkeeping": false,
      "params": [
        "f64",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v38"
      ],
      "capability": "kv.read",
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "kv_open",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v38"
      ],
      "capability": "kv.read",
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "kv_read",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "kv_stage",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": true,
      "minHostAbi": 34,
      "name": "kv_text",
      "networkBookkeeping": false,
      "params": [],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "kv_value",
      "networkBookkeeping": false,
      "params": [
        "f64",
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf8-v1",
        "checked-f64-utf8-v2",
        "checked-f64-utf16-v1",
        "checked-f64-utf16-v2",
        "checked-f64-utf16-v3",
        "checked-f64-utf16-v4",
        "checked-f64-utf16-v5",
        "checked-f64-utf16-v6",
        "checked-f64-utf16-v7",
        "checked-f64-utf16-v8",
        "checked-f64-utf16-v9",
        "checked-f64-utf16-v10",
        "checked-f64-utf16-v11",
        "checked-f64-utf16-v12",
        "checked-f64-utf16-v13",
        "checked-f64-utf16-v14",
        "checked-f64-utf16-v15",
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 2,
      "name": "log_f64",
      "networkBookkeeping": false,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37",
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 21,
      "name": "log_f64_v2",
      "networkBookkeeping": false,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-i32-utf8-v1",
        "checked-f64-utf8-v1",
        "checked-f64-utf8-v2",
        "checked-f64-utf16-v1",
        "checked-f64-utf16-v2",
        "checked-f64-utf16-v3",
        "checked-f64-utf16-v4",
        "checked-f64-utf16-v5",
        "checked-f64-utf16-v6",
        "checked-f64-utf16-v7",
        "checked-f64-utf16-v8",
        "checked-f64-utf16-v9",
        "checked-f64-utf16-v10",
        "checked-f64-utf16-v11",
        "checked-f64-utf16-v12",
        "checked-f64-utf16-v13",
        "checked-f64-utf16-v14",
        "checked-f64-utf16-v15",
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37",
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 1,
      "name": "log_i32",
      "networkBookkeeping": false,
      "params": [
        "i32"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "log_stderr16",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-i32-utf8-v1",
        "checked-f64-utf8-v1",
        "checked-f64-utf8-v2"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 1,
      "name": "log_string",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v1",
        "checked-f64-utf16-v2",
        "checked-f64-utf16-v3",
        "checked-f64-utf16-v4",
        "checked-f64-utf16-v5",
        "checked-f64-utf16-v6",
        "checked-f64-utf16-v7",
        "checked-f64-utf16-v8",
        "checked-f64-utf16-v9",
        "checked-f64-utf16-v10",
        "checked-f64-utf16-v11",
        "checked-f64-utf16-v12",
        "checked-f64-utf16-v13",
        "checked-f64-utf16-v14",
        "checked-f64-utf16-v15",
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37",
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 4,
      "name": "log_string16",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf8-v2",
        "checked-f64-utf16-v1",
        "checked-f64-utf16-v2",
        "checked-f64-utf16-v3",
        "checked-f64-utf16-v4",
        "checked-f64-utf16-v5",
        "checked-f64-utf16-v6",
        "checked-f64-utf16-v7",
        "checked-f64-utf16-v8",
        "checked-f64-utf16-v9",
        "checked-f64-utf16-v10",
        "checked-f64-utf16-v11",
        "checked-f64-utf16-v12",
        "checked-f64-utf16-v13",
        "checked-f64-utf16-v14",
        "checked-f64-utf16-v15",
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37",
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 3,
      "name": "log_undefined",
      "networkBookkeeping": false,
      "params": [
        "i32"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v37",
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 33,
      "name": "net_body_read",
      "networkBookkeeping": true,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "net_body_read_bytes",
      "networkBookkeeping": true,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-i32-utf8-v1",
        "checked-f64-utf8-v1",
        "checked-f64-utf8-v2"
      ],
      "capability": "net.fetch",
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 1,
      "name": "net_fetch",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v1",
        "checked-f64-utf16-v2",
        "checked-f64-utf16-v3",
        "checked-f64-utf16-v4",
        "checked-f64-utf16-v5",
        "checked-f64-utf16-v6",
        "checked-f64-utf16-v7",
        "checked-f64-utf16-v8",
        "checked-f64-utf16-v9",
        "checked-f64-utf16-v10",
        "checked-f64-utf16-v11",
        "checked-f64-utf16-v12",
        "checked-f64-utf16-v13",
        "checked-f64-utf16-v14",
        "checked-f64-utf16-v15",
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37",
        "checked-f64-utf16-v38"
      ],
      "capability": "net.fetch",
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 4,
      "name": "net_fetch16",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-i32-utf8-v1",
        "checked-f64-utf8-v1",
        "checked-f64-utf8-v2"
      ],
      "capability": "net.get",
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 1,
      "name": "net_get",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v1",
        "checked-f64-utf16-v2",
        "checked-f64-utf16-v3",
        "checked-f64-utf16-v4",
        "checked-f64-utf16-v5",
        "checked-f64-utf16-v6",
        "checked-f64-utf16-v7",
        "checked-f64-utf16-v8",
        "checked-f64-utf16-v9",
        "checked-f64-utf16-v10",
        "checked-f64-utf16-v11",
        "checked-f64-utf16-v12",
        "checked-f64-utf16-v13",
        "checked-f64-utf16-v14",
        "checked-f64-utf16-v15",
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37",
        "checked-f64-utf16-v38"
      ],
      "capability": "net.get",
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 4,
      "name": "net_get16",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v37",
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 33,
      "name": "net_request_abort",
      "networkBookkeeping": true,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v37"
      ],
      "capability": "net.request",
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 33,
      "name": "net_request_start",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v38"
      ],
      "capability": "net.request",
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "net_request_start_v2",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v37",
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 33,
      "name": "net_request_take",
      "networkBookkeeping": true,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v37",
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 33,
      "name": "net_request_text",
      "networkBookkeeping": true,
      "params": [
        "f64"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v37",
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 33,
      "name": "net_request_watch",
      "networkBookkeeping": true,
      "params": [
        "f64",
        "f64"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 19,
      "name": "number_acos",
      "networkBookkeeping": false,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 19,
      "name": "number_acosh",
      "networkBookkeeping": false,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 19,
      "name": "number_asin",
      "networkBookkeeping": false,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 19,
      "name": "number_asinh",
      "networkBookkeeping": false,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 19,
      "name": "number_atan",
      "networkBookkeeping": false,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 19,
      "name": "number_atan2",
      "networkBookkeeping": false,
      "params": [
        "f64",
        "f64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 19,
      "name": "number_atanh",
      "networkBookkeeping": false,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 19,
      "name": "number_cbrt",
      "networkBookkeeping": false,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 19,
      "name": "number_cos",
      "networkBookkeeping": false,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 19,
      "name": "number_cosh",
      "networkBookkeeping": false,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 19,
      "name": "number_exp",
      "networkBookkeeping": false,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 19,
      "name": "number_expm1",
      "networkBookkeeping": false,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 19,
      "name": "number_log",
      "networkBookkeeping": false,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 19,
      "name": "number_log10",
      "networkBookkeeping": false,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 19,
      "name": "number_log1p",
      "networkBookkeeping": false,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 19,
      "name": "number_log2",
      "networkBookkeeping": false,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 26,
      "name": "number_parse_float",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 26,
      "name": "number_parse_int",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v15",
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 18,
      "name": "number_power",
      "networkBookkeeping": false,
      "params": [
        "f64",
        "f64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37",
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 20,
      "name": "number_random",
      "networkBookkeeping": false,
      "params": [],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf8-v1",
        "checked-f64-utf8-v2",
        "checked-f64-utf16-v1",
        "checked-f64-utf16-v2",
        "checked-f64-utf16-v3",
        "checked-f64-utf16-v4",
        "checked-f64-utf16-v5",
        "checked-f64-utf16-v6",
        "checked-f64-utf16-v7",
        "checked-f64-utf16-v8",
        "checked-f64-utf16-v9",
        "checked-f64-utf16-v10",
        "checked-f64-utf16-v11",
        "checked-f64-utf16-v12",
        "checked-f64-utf16-v13",
        "checked-f64-utf16-v14",
        "checked-f64-utf16-v15",
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 2,
      "name": "number_remainder",
      "networkBookkeeping": false,
      "params": [
        "f64",
        "f64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 19,
      "name": "number_sin",
      "networkBookkeeping": false,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 19,
      "name": "number_sinh",
      "networkBookkeeping": false,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 19,
      "name": "number_tan",
      "networkBookkeeping": false,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 19,
      "name": "number_tanh",
      "networkBookkeeping": false,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 25,
      "name": "number_to_exponential",
      "networkBookkeeping": false,
      "params": [
        "f64",
        "f64"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 25,
      "name": "number_to_fixed",
      "networkBookkeeping": false,
      "params": [
        "f64",
        "f64"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 25,
      "name": "number_to_precision",
      "networkBookkeeping": false,
      "params": [
        "f64",
        "f64"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 24,
      "name": "number_to_string",
      "networkBookkeeping": false,
      "params": [
        "f64",
        "f64"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": true,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "permission_query",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": true,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "permission_request",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 28,
      "name": "primitive_compare",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 28,
      "name": "primitive_equal",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i64",
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 23,
      "name": "primitive_to_number",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 28,
      "name": "primitive_to_number_v2",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v34"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 30,
      "name": "primitive_to_number_v3",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v35"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 31,
      "name": "primitive_to_number_v4",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 32,
      "name": "primitive_to_number_v5",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "primitive_to_number_v6",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 22,
      "name": "primitive_to_string",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 28,
      "name": "primitive_to_string_v2",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v34"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 30,
      "name": "primitive_to_string_v3",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v35"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 31,
      "name": "primitive_to_string_v4",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 32,
      "name": "primitive_to_string_v5",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "primitive_to_string_v6",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-i32-utf8-v1",
        "checked-f64-utf8-v1",
        "checked-f64-utf8-v2",
        "checked-f64-utf16-v1",
        "checked-f64-utf16-v2",
        "checked-f64-utf16-v3",
        "checked-f64-utf16-v4",
        "checked-f64-utf16-v5",
        "checked-f64-utf16-v6",
        "checked-f64-utf16-v7",
        "checked-f64-utf16-v8",
        "checked-f64-utf16-v9",
        "checked-f64-utf16-v10",
        "checked-f64-utf16-v11",
        "checked-f64-utf16-v12",
        "checked-f64-utf16-v13",
        "checked-f64-utf16-v14",
        "checked-f64-utf16-v15",
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37",
        "checked-f64-utf16-v38"
      ],
      "capability": "process.exit",
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 1,
      "name": "process_exit",
      "networkBookkeeping": false,
      "params": [
        "i32"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf8-v1",
        "checked-f64-utf8-v2",
        "checked-f64-utf16-v1",
        "checked-f64-utf16-v2",
        "checked-f64-utf16-v3",
        "checked-f64-utf16-v4",
        "checked-f64-utf16-v5",
        "checked-f64-utf16-v6",
        "checked-f64-utf16-v7",
        "checked-f64-utf16-v8",
        "checked-f64-utf16-v9",
        "checked-f64-utf16-v10",
        "checked-f64-utf16-v11",
        "checked-f64-utf16-v12",
        "checked-f64-utf16-v13",
        "checked-f64-utf16-v14",
        "checked-f64-utf16-v15",
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37",
        "checked-f64-utf16-v38"
      ],
      "capability": "process.exit",
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 2,
      "name": "process_exit_f64",
      "networkBookkeeping": false,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37",
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 30,
      "name": "promise_handled",
      "networkBookkeeping": false,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": true,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37",
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 30,
      "name": "promise_react",
      "networkBookkeeping": false,
      "params": [
        "f64",
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": true,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37",
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 30,
      "name": "promise_settle",
      "networkBookkeeping": false,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": true,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37",
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 30,
      "name": "promise_unhandled",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": true,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "regexp_exec",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "i32",
        "i32",
        "f64",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37",
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 30,
      "name": "scheduler_enqueue",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": true,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v1",
        "checked-f64-utf16-v2",
        "checked-f64-utf16-v3",
        "checked-f64-utf16-v4",
        "checked-f64-utf16-v5",
        "checked-f64-utf16-v6",
        "checked-f64-utf16-v7",
        "checked-f64-utf16-v8",
        "checked-f64-utf16-v9",
        "checked-f64-utf16-v10",
        "checked-f64-utf16-v11",
        "checked-f64-utf16-v12",
        "checked-f64-utf16-v13",
        "checked-f64-utf16-v14",
        "checked-f64-utf16-v15",
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 4,
      "name": "string_char_at16",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "f64"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v1",
        "checked-f64-utf16-v2",
        "checked-f64-utf16-v3",
        "checked-f64-utf16-v4",
        "checked-f64-utf16-v5",
        "checked-f64-utf16-v6",
        "checked-f64-utf16-v7",
        "checked-f64-utf16-v8",
        "checked-f64-utf16-v9",
        "checked-f64-utf16-v10",
        "checked-f64-utf16-v11",
        "checked-f64-utf16-v12",
        "checked-f64-utf16-v13",
        "checked-f64-utf16-v14",
        "checked-f64-utf16-v15",
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 4,
      "name": "string_char_code_at16",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "f64"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "string_normalize16",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32",
        "f64"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 33,
      "name": "string_to_lower_case16",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 33,
      "name": "string_to_upper_case16",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-i32-utf8-v1",
        "checked-f64-utf8-v1",
        "checked-f64-utf8-v2",
        "checked-f64-utf16-v1",
        "checked-f64-utf16-v2",
        "checked-f64-utf16-v3",
        "checked-f64-utf16-v4",
        "checked-f64-utf16-v5",
        "checked-f64-utf16-v6",
        "checked-f64-utf16-v7",
        "checked-f64-utf16-v8",
        "checked-f64-utf16-v9",
        "checked-f64-utf16-v10",
        "checked-f64-utf16-v11",
        "checked-f64-utf16-v12",
        "checked-f64-utf16-v13",
        "checked-f64-utf16-v14",
        "checked-f64-utf16-v15",
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28",
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33",
        "checked-f64-utf16-v34",
        "checked-f64-utf16-v35",
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37",
        "checked-f64-utf16-v38"
      ],
      "capability": "time.now",
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 1,
      "name": "time_now",
      "networkBookkeeping": false,
      "params": [],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37",
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 32,
      "name": "timer_clear",
      "networkBookkeeping": false,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": true
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v36"
      ],
      "capability": "time.timer",
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 32,
      "name": "timer_start",
      "networkBookkeeping": false,
      "params": [
        "f64",
        "f64",
        "i32"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v37",
        "checked-f64-utf16-v38"
      ],
      "capability": "time.timer",
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 33,
      "name": "timer_start_v2",
      "networkBookkeeping": false,
      "params": [
        "f64",
        "f64",
        "i32"
      ],
      "resultGlobals": false,
      "results": [
        "f64"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37",
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 32,
      "name": "timer_take",
      "networkBookkeeping": false,
      "params": [
        "f64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": true
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37",
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 32,
      "name": "timer_uncaught",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": true
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v2",
        "checked-f64-utf16-v3"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 5,
      "name": "union_log",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v2",
        "checked-f64-utf16-v3"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 5,
      "name": "union_nullish",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v2",
        "checked-f64-utf16-v3"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 5,
      "name": "union_nullish_equal",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v2",
        "checked-f64-utf16-v3"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 5,
      "name": "union_strict_equal",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v2",
        "checked-f64-utf16-v3"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 5,
      "name": "union_truthy",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v2",
        "checked-f64-utf16-v3"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 5,
      "name": "union_typeof",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "value_format_v12",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 32,
      "name": "value_is_array_v10",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "value_is_array_v11",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v4",
        "checked-f64-utf16-v5"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 7,
      "name": "value_is_array_v2",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v6",
        "checked-f64-utf16-v7"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 9,
      "name": "value_is_array_v3",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v8"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 11,
      "name": "value_is_array_v4",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v9",
        "checked-f64-utf16-v10",
        "checked-f64-utf16-v11",
        "checked-f64-utf16-v12"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 12,
      "name": "value_is_array_v5",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v13",
        "checked-f64-utf16-v14",
        "checked-f64-utf16-v15",
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 16,
      "name": "value_is_array_v6",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 28,
      "name": "value_is_array_v7",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v34"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 30,
      "name": "value_is_array_v8",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v35"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 31,
      "name": "value_is_array_v9",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v8"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 11,
      "name": "value_is_error",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v9",
        "checked-f64-utf16-v10",
        "checked-f64-utf16-v11",
        "checked-f64-utf16-v12"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 12,
      "name": "value_is_error_class",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i32"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v13",
        "checked-f64-utf16-v14",
        "checked-f64-utf16-v15",
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 16,
      "name": "value_is_error_class_v2",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i32"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 28,
      "name": "value_is_error_class_v3",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i32"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v34"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 30,
      "name": "value_is_error_class_v4",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i32"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v35"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 31,
      "name": "value_is_error_class_v5",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i32"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 32,
      "name": "value_is_error_class_v6",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i32"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "value_is_error_class_v7",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i32"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v35"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 31,
      "name": "value_log_v10",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 32,
      "name": "value_log_v11",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "value_log_v12",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v4",
        "checked-f64-utf16-v5"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 7,
      "name": "value_log_v2",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v6",
        "checked-f64-utf16-v7"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 9,
      "name": "value_log_v3",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v8"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 11,
      "name": "value_log_v4",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v9",
        "checked-f64-utf16-v10",
        "checked-f64-utf16-v11",
        "checked-f64-utf16-v12"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 12,
      "name": "value_log_v5",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v13",
        "checked-f64-utf16-v14",
        "checked-f64-utf16-v15",
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 16,
      "name": "value_log_v6",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 21,
      "name": "value_log_v7",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 28,
      "name": "value_log_v8",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v34"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 30,
      "name": "value_log_v9",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 32,
      "name": "value_nullish_equal_v10",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "value_nullish_equal_v11",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v4",
        "checked-f64-utf16-v5"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 7,
      "name": "value_nullish_equal_v2",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v6",
        "checked-f64-utf16-v7"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 9,
      "name": "value_nullish_equal_v3",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v8"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 11,
      "name": "value_nullish_equal_v4",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v9",
        "checked-f64-utf16-v10",
        "checked-f64-utf16-v11",
        "checked-f64-utf16-v12"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 12,
      "name": "value_nullish_equal_v5",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v13",
        "checked-f64-utf16-v14",
        "checked-f64-utf16-v15",
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 16,
      "name": "value_nullish_equal_v6",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 28,
      "name": "value_nullish_equal_v7",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v34"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 30,
      "name": "value_nullish_equal_v8",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v35"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 31,
      "name": "value_nullish_equal_v9",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 32,
      "name": "value_nullish_v10",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "value_nullish_v11",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v4",
        "checked-f64-utf16-v5"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 7,
      "name": "value_nullish_v2",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v6",
        "checked-f64-utf16-v7"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 9,
      "name": "value_nullish_v3",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v8"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 11,
      "name": "value_nullish_v4",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v9",
        "checked-f64-utf16-v10",
        "checked-f64-utf16-v11",
        "checked-f64-utf16-v12"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 12,
      "name": "value_nullish_v5",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v13",
        "checked-f64-utf16-v14",
        "checked-f64-utf16-v15",
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 16,
      "name": "value_nullish_v6",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 28,
      "name": "value_nullish_v7",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v34"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 30,
      "name": "value_nullish_v8",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v35"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 31,
      "name": "value_nullish_v9",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 32,
      "name": "value_strict_equal_v10",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "value_strict_equal_v11",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v4",
        "checked-f64-utf16-v5"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 7,
      "name": "value_strict_equal_v2",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v6",
        "checked-f64-utf16-v7"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 9,
      "name": "value_strict_equal_v3",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v8"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 11,
      "name": "value_strict_equal_v4",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v9",
        "checked-f64-utf16-v10",
        "checked-f64-utf16-v11",
        "checked-f64-utf16-v12"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 12,
      "name": "value_strict_equal_v5",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v13",
        "checked-f64-utf16-v14",
        "checked-f64-utf16-v15",
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 16,
      "name": "value_strict_equal_v6",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 28,
      "name": "value_strict_equal_v7",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v34"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 30,
      "name": "value_strict_equal_v8",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v35"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 31,
      "name": "value_strict_equal_v9",
      "networkBookkeeping": false,
      "params": [
        "i64",
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 32,
      "name": "value_truthy_v10",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "value_truthy_v11",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v4",
        "checked-f64-utf16-v5"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 7,
      "name": "value_truthy_v2",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v6",
        "checked-f64-utf16-v7"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 9,
      "name": "value_truthy_v3",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v8"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 11,
      "name": "value_truthy_v4",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v9",
        "checked-f64-utf16-v10",
        "checked-f64-utf16-v11",
        "checked-f64-utf16-v12"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 12,
      "name": "value_truthy_v5",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v13",
        "checked-f64-utf16-v14",
        "checked-f64-utf16-v15",
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 16,
      "name": "value_truthy_v6",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 28,
      "name": "value_truthy_v7",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v34"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 30,
      "name": "value_truthy_v8",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v35"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 31,
      "name": "value_truthy_v9",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v36",
        "checked-f64-utf16-v37"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 32,
      "name": "value_typeof_v10",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "value_typeof_v11",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v4",
        "checked-f64-utf16-v5"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 7,
      "name": "value_typeof_v2",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v6",
        "checked-f64-utf16-v7"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 9,
      "name": "value_typeof_v3",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v8"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 11,
      "name": "value_typeof_v4",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v9",
        "checked-f64-utf16-v10",
        "checked-f64-utf16-v11",
        "checked-f64-utf16-v12"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 12,
      "name": "value_typeof_v5",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v13",
        "checked-f64-utf16-v14",
        "checked-f64-utf16-v15",
        "checked-f64-utf16-v16",
        "checked-f64-utf16-v17",
        "checked-f64-utf16-v18",
        "checked-f64-utf16-v19",
        "checked-f64-utf16-v20",
        "checked-f64-utf16-v21",
        "checked-f64-utf16-v22",
        "checked-f64-utf16-v23",
        "checked-f64-utf16-v24",
        "checked-f64-utf16-v25",
        "checked-f64-utf16-v26",
        "checked-f64-utf16-v27",
        "checked-f64-utf16-v28"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 16,
      "name": "value_typeof_v6",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v29",
        "checked-f64-utf16-v30",
        "checked-f64-utf16-v31",
        "checked-f64-utf16-v32",
        "checked-f64-utf16-v33"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 28,
      "name": "value_typeof_v7",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v34"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 30,
      "name": "value_typeof_v8",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v35"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 31,
      "name": "value_typeof_v9",
      "networkBookkeeping": false,
      "params": [
        "i64"
      ],
      "resultGlobals": true,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": true,
      "timerBookkeeping": false
    },
    {
      "admittedUnder": [
        "checked-f64-utf16-v38"
      ],
      "capability": null,
      "instanceBookkeeping": false,
      "kvBookkeeping": false,
      "minHostAbi": 34,
      "name": "view_commit",
      "networkBookkeeping": false,
      "params": [
        "i32",
        "i32"
      ],
      "resultGlobals": false,
      "results": [
        "i32"
      ],
      "scheduler": false,
      "stringResult": false,
      "timerBookkeeping": false
    }
  ],
  "permissions": {
    "artifactHex": 64,
    "artifactPrefix": "sha256:",
    "capabilities": [
      {
        "id": "clipboard.write",
        "kind": "boolean"
      },
      {
        "id": "env.get",
        "kind": "patterns"
      },
      {
        "id": "env.has",
        "kind": "patterns"
      },
      {
        "id": "env.print",
        "kind": "patterns"
      },
      {
        "id": "fs.print",
        "kind": "patterns"
      },
      {
        "id": "fs.read",
        "kind": "patterns"
      },
      {
        "id": "kv.read",
        "kind": "patterns"
      },
      {
        "id": "kv.write",
        "kind": "patterns"
      },
      {
        "id": "net.fetch",
        "kind": "urls"
      },
      {
        "id": "net.get",
        "kind": "urls"
      },
      {
        "id": "net.listen",
        "kind": "listeners"
      },
      {
        "id": "net.request",
        "kind": "requests"
      },
      {
        "id": "process.exit",
        "kind": "boolean"
      },
      {
        "id": "time.now",
        "kind": "boolean"
      },
      {
        "id": "time.timer",
        "kind": "boolean"
      },
      {
        "id": "ui.open_url",
        "kind": "openUrls"
      }
    ],
    "decisions": [
      "allow",
      "deny"
    ],
    "descriptor": {
      "fields": 4,
      "invalid": 0,
      "maxBytes": 16384,
      "none": "0",
      "resource": "1",
      "separator": "\n",
      "version": "varta-permission-1"
    },
    "events": {
      "closed": 2,
      "event": 1,
      "invalid": 0.0,
      "maxId": 9007199254740991,
      "nameBytes": 64,
      "noListener": "no open host event source is named {name}",
      "queueError": "host task queue is full (256 tasks)",
      "recordBytes": 65536,
      "recordError": "event record exceeds 65536 bytes",
      "sources": 32,
      "stateError": "invalid host event source state",
      "tasks": 256
    },
    "limits": {
      "applications": 64,
      "audit": 256,
      "auditResourceBytes": 512,
      "decisions": 128,
      "dismissals": 3,
      "prompts": 1
    },
    "requiredDenied": "required permission {id} is denied for this application (stored decision); the application cannot run",
    "requiredRefused": "required permission {id} is denied for this application (refused at first run); the application cannot run",
    "schemaVersion": 1,
    "states": {
      "denied": 2,
      "granted": 1,
      "prompt": 3
    }
  },
  "profiles": [
    {
      "acceptance": {
        "catalogs": [],
        "functionLimitCountsImports": false,
        "helperAllowance": 10,
        "maxDefinedFunctions": 267,
        "values": [
          1,
          2
        ]
      },
      "admittedImports": 15,
      "exceptionChannel": false,
      "exceptionVersion": null,
      "fairFuel": false,
      "frames": null,
      "hostAbi": 1,
      "httpVersion": 1,
      "name": "checked-i32-utf8-v1",
      "numbers": "i32",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-8",
      "supportedKinds": [
        0,
        1,
        2,
        3,
        4,
        5
      ],
      "toolEntries": true,
      "valueAbi": 2
    },
    {
      "acceptance": {
        "catalogs": [],
        "functionLimitCountsImports": false,
        "helperAllowance": 10,
        "maxDefinedFunctions": 267,
        "values": [
          3
        ]
      },
      "admittedImports": 17,
      "exceptionChannel": false,
      "exceptionVersion": null,
      "fairFuel": false,
      "frames": null,
      "hostAbi": 2,
      "httpVersion": null,
      "name": "checked-f64-utf8-v1",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-8",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6
      ],
      "toolEntries": false,
      "valueAbi": 3
    },
    {
      "acceptance": {
        "catalogs": [],
        "functionLimitCountsImports": false,
        "helperAllowance": 10,
        "maxDefinedFunctions": 267,
        "values": [
          4
        ]
      },
      "admittedImports": 18,
      "exceptionChannel": false,
      "exceptionVersion": null,
      "fairFuel": false,
      "frames": null,
      "hostAbi": 3,
      "httpVersion": null,
      "name": "checked-f64-utf8-v2",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-8",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7
      ],
      "toolEntries": false,
      "valueAbi": 4
    },
    {
      "acceptance": {
        "catalogs": [],
        "functionLimitCountsImports": false,
        "helperAllowance": 10,
        "maxDefinedFunctions": 267,
        "values": [
          5
        ]
      },
      "admittedImports": 20,
      "exceptionChannel": false,
      "exceptionVersion": null,
      "fairFuel": false,
      "frames": null,
      "hostAbi": 4,
      "httpVersion": null,
      "name": "checked-f64-utf16-v1",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7
      ],
      "toolEntries": false,
      "valueAbi": 5
    },
    {
      "acceptance": {
        "catalogs": [],
        "functionLimitCountsImports": false,
        "helperAllowance": 10,
        "maxDefinedFunctions": 267,
        "values": [
          6
        ]
      },
      "admittedImports": 26,
      "exceptionChannel": false,
      "exceptionVersion": null,
      "fairFuel": false,
      "frames": null,
      "hostAbi": 5,
      "httpVersion": null,
      "name": "checked-f64-utf16-v2",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8
      ],
      "toolEntries": false,
      "valueAbi": 6
    },
    {
      "acceptance": {
        "catalogs": [],
        "functionLimitCountsImports": false,
        "helperAllowance": 10,
        "maxDefinedFunctions": 267,
        "values": [
          7
        ]
      },
      "admittedImports": 26,
      "exceptionChannel": false,
      "exceptionVersion": null,
      "fairFuel": false,
      "frames": null,
      "hostAbi": 6,
      "httpVersion": null,
      "name": "checked-f64-utf16-v3",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8
      ],
      "toolEntries": false,
      "valueAbi": 7
    },
    {
      "acceptance": {
        "catalogs": [],
        "functionLimitCountsImports": false,
        "helperAllowance": 10,
        "maxDefinedFunctions": 267,
        "values": [
          8
        ]
      },
      "admittedImports": 27,
      "exceptionChannel": false,
      "exceptionVersion": null,
      "fairFuel": false,
      "frames": null,
      "hostAbi": 7,
      "httpVersion": null,
      "name": "checked-f64-utf16-v4",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9
      ],
      "toolEntries": false,
      "valueAbi": 8
    },
    {
      "acceptance": {
        "catalogs": [],
        "functionLimitCountsImports": false,
        "helperAllowance": 10,
        "maxDefinedFunctions": 267,
        "values": [
          8
        ]
      },
      "admittedImports": 27,
      "exceptionChannel": false,
      "exceptionVersion": null,
      "fairFuel": false,
      "frames": null,
      "hostAbi": 8,
      "httpVersion": null,
      "name": "checked-f64-utf16-v5",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9
      ],
      "toolEntries": false,
      "valueAbi": 8
    },
    {
      "acceptance": {
        "catalogs": [],
        "functionLimitCountsImports": false,
        "helperAllowance": 10,
        "maxDefinedFunctions": 267,
        "values": [
          9
        ]
      },
      "admittedImports": 27,
      "exceptionChannel": false,
      "exceptionVersion": null,
      "fairFuel": false,
      "frames": null,
      "hostAbi": 9,
      "httpVersion": null,
      "name": "checked-f64-utf16-v6",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9
      ],
      "toolEntries": false,
      "valueAbi": 9
    },
    {
      "acceptance": {
        "catalogs": [],
        "functionLimitCountsImports": false,
        "helperAllowance": 10,
        "maxDefinedFunctions": 267,
        "values": [
          9
        ]
      },
      "admittedImports": 27,
      "exceptionChannel": true,
      "exceptionVersion": null,
      "fairFuel": false,
      "frames": null,
      "hostAbi": 10,
      "httpVersion": null,
      "name": "checked-f64-utf16-v7",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9
      ],
      "toolEntries": false,
      "valueAbi": 9
    },
    {
      "acceptance": {
        "catalogs": [],
        "functionLimitCountsImports": false,
        "helperAllowance": 10,
        "maxDefinedFunctions": 267,
        "values": [
          10
        ]
      },
      "admittedImports": 28,
      "exceptionChannel": true,
      "exceptionVersion": {
        "global": "varta.exception.version",
        "value": 2
      },
      "fairFuel": false,
      "frames": null,
      "hostAbi": 11,
      "httpVersion": null,
      "name": "checked-f64-utf16-v8",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10
      ],
      "toolEntries": false,
      "valueAbi": 10
    },
    {
      "acceptance": {
        "catalogs": [],
        "functionLimitCountsImports": false,
        "helperAllowance": 10,
        "maxDefinedFunctions": 267,
        "values": [
          11
        ]
      },
      "admittedImports": 28,
      "exceptionChannel": true,
      "exceptionVersion": {
        "global": "varta.exception.version_v3",
        "value": 3
      },
      "fairFuel": false,
      "frames": null,
      "hostAbi": 12,
      "httpVersion": null,
      "name": "checked-f64-utf16-v9",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10
      ],
      "toolEntries": false,
      "valueAbi": 11
    },
    {
      "acceptance": {
        "catalogs": [],
        "functionLimitCountsImports": false,
        "helperAllowance": 10,
        "maxDefinedFunctions": 267,
        "values": [
          11
        ]
      },
      "admittedImports": 28,
      "exceptionChannel": true,
      "exceptionVersion": {
        "global": "varta.exception.version_v3",
        "value": 3
      },
      "fairFuel": false,
      "frames": null,
      "hostAbi": 13,
      "httpVersion": null,
      "name": "checked-f64-utf16-v10",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10
      ],
      "toolEntries": false,
      "valueAbi": 11
    },
    {
      "acceptance": {
        "catalogs": [],
        "functionLimitCountsImports": false,
        "helperAllowance": 10,
        "maxDefinedFunctions": 267,
        "values": [
          11
        ]
      },
      "admittedImports": 28,
      "exceptionChannel": true,
      "exceptionVersion": {
        "global": "varta.exception.version_v3",
        "value": 3
      },
      "fairFuel": false,
      "frames": null,
      "hostAbi": 14,
      "httpVersion": null,
      "name": "checked-f64-utf16-v11",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10
      ],
      "toolEntries": false,
      "valueAbi": 11
    },
    {
      "acceptance": {
        "catalogs": [],
        "functionLimitCountsImports": false,
        "helperAllowance": 10,
        "maxDefinedFunctions": 267,
        "values": [
          11
        ]
      },
      "admittedImports": 28,
      "exceptionChannel": true,
      "exceptionVersion": {
        "global": "varta.exception.version_v3",
        "value": 3
      },
      "fairFuel": false,
      "frames": null,
      "hostAbi": 15,
      "httpVersion": null,
      "name": "checked-f64-utf16-v12",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10
      ],
      "toolEntries": false,
      "valueAbi": 11
    },
    {
      "acceptance": {
        "catalogs": [],
        "functionLimitCountsImports": false,
        "helperAllowance": 12,
        "maxDefinedFunctions": 269,
        "values": [
          12
        ]
      },
      "admittedImports": 28,
      "exceptionChannel": true,
      "exceptionVersion": {
        "global": "varta.exception.version_v3",
        "value": 3
      },
      "fairFuel": false,
      "frames": null,
      "hostAbi": 16,
      "httpVersion": null,
      "name": "checked-f64-utf16-v13",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10
      ],
      "toolEntries": false,
      "valueAbi": 12
    },
    {
      "acceptance": {
        "catalogs": [],
        "functionLimitCountsImports": false,
        "helperAllowance": 13,
        "maxDefinedFunctions": 270,
        "values": [
          12
        ]
      },
      "admittedImports": 28,
      "exceptionChannel": true,
      "exceptionVersion": {
        "global": "varta.exception.version_v3",
        "value": 3
      },
      "fairFuel": false,
      "frames": null,
      "hostAbi": 17,
      "httpVersion": null,
      "name": "checked-f64-utf16-v14",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10
      ],
      "toolEntries": false,
      "valueAbi": 12
    },
    {
      "acceptance": {
        "catalogs": [],
        "functionLimitCountsImports": false,
        "helperAllowance": 13,
        "maxDefinedFunctions": 270,
        "values": [
          12
        ]
      },
      "admittedImports": 29,
      "exceptionChannel": true,
      "exceptionVersion": {
        "global": "varta.exception.version_v3",
        "value": 3
      },
      "fairFuel": false,
      "frames": null,
      "hostAbi": 18,
      "httpVersion": null,
      "name": "checked-f64-utf16-v15",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10
      ],
      "toolEntries": false,
      "valueAbi": 12
    },
    {
      "acceptance": {
        "catalogs": [],
        "functionLimitCountsImports": false,
        "helperAllowance": 13,
        "maxDefinedFunctions": 270,
        "values": [
          12
        ]
      },
      "admittedImports": 49,
      "exceptionChannel": true,
      "exceptionVersion": {
        "global": "varta.exception.version_v3",
        "value": 3
      },
      "fairFuel": false,
      "frames": null,
      "hostAbi": 19,
      "httpVersion": null,
      "name": "checked-f64-utf16-v16",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10
      ],
      "toolEntries": false,
      "valueAbi": 12
    },
    {
      "acceptance": {
        "catalogs": [],
        "functionLimitCountsImports": false,
        "helperAllowance": 13,
        "maxDefinedFunctions": 270,
        "values": [
          12
        ]
      },
      "admittedImports": 50,
      "exceptionChannel": true,
      "exceptionVersion": {
        "global": "varta.exception.version_v3",
        "value": 3
      },
      "fairFuel": false,
      "frames": null,
      "hostAbi": 20,
      "httpVersion": null,
      "name": "checked-f64-utf16-v17",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10
      ],
      "toolEntries": false,
      "valueAbi": 12
    },
    {
      "acceptance": {
        "catalogs": [],
        "functionLimitCountsImports": false,
        "helperAllowance": 13,
        "maxDefinedFunctions": 270,
        "values": [
          12
        ]
      },
      "admittedImports": 50,
      "exceptionChannel": true,
      "exceptionVersion": {
        "global": "varta.exception.version_v3",
        "value": 3
      },
      "fairFuel": false,
      "frames": null,
      "hostAbi": 21,
      "httpVersion": null,
      "name": "checked-f64-utf16-v18",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10
      ],
      "toolEntries": false,
      "valueAbi": 12
    },
    {
      "acceptance": {
        "catalogs": [],
        "functionLimitCountsImports": false,
        "helperAllowance": 13,
        "maxDefinedFunctions": 270,
        "values": [
          12
        ]
      },
      "admittedImports": 51,
      "exceptionChannel": true,
      "exceptionVersion": {
        "global": "varta.exception.version_v3",
        "value": 3
      },
      "fairFuel": false,
      "frames": null,
      "hostAbi": 22,
      "httpVersion": null,
      "name": "checked-f64-utf16-v19",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10
      ],
      "toolEntries": false,
      "valueAbi": 12
    },
    {
      "acceptance": {
        "catalogs": [],
        "functionLimitCountsImports": false,
        "helperAllowance": 13,
        "maxDefinedFunctions": 270,
        "values": [
          12
        ]
      },
      "admittedImports": 52,
      "exceptionChannel": true,
      "exceptionVersion": {
        "global": "varta.exception.version_v3",
        "value": 3
      },
      "fairFuel": false,
      "frames": null,
      "hostAbi": 23,
      "httpVersion": null,
      "name": "checked-f64-utf16-v20",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10
      ],
      "toolEntries": false,
      "valueAbi": 12
    },
    {
      "acceptance": {
        "catalogs": [],
        "functionLimitCountsImports": false,
        "helperAllowance": 13,
        "maxDefinedFunctions": 270,
        "values": [
          12
        ]
      },
      "admittedImports": 53,
      "exceptionChannel": true,
      "exceptionVersion": {
        "global": "varta.exception.version_v3",
        "value": 3
      },
      "fairFuel": false,
      "frames": null,
      "hostAbi": 24,
      "httpVersion": null,
      "name": "checked-f64-utf16-v21",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10
      ],
      "toolEntries": false,
      "valueAbi": 12
    },
    {
      "acceptance": {
        "catalogs": [],
        "functionLimitCountsImports": false,
        "helperAllowance": 13,
        "maxDefinedFunctions": 270,
        "values": [
          12
        ]
      },
      "admittedImports": 56,
      "exceptionChannel": true,
      "exceptionVersion": {
        "global": "varta.exception.version_v3",
        "value": 3
      },
      "fairFuel": false,
      "frames": null,
      "hostAbi": 25,
      "httpVersion": null,
      "name": "checked-f64-utf16-v22",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10
      ],
      "toolEntries": false,
      "valueAbi": 12
    },
    {
      "acceptance": {
        "catalogs": [],
        "functionLimitCountsImports": false,
        "helperAllowance": 13,
        "maxDefinedFunctions": 270,
        "values": [
          12
        ]
      },
      "admittedImports": 58,
      "exceptionChannel": true,
      "exceptionVersion": {
        "global": "varta.exception.version_v3",
        "value": 3
      },
      "fairFuel": false,
      "frames": null,
      "hostAbi": 26,
      "httpVersion": null,
      "name": "checked-f64-utf16-v23",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10
      ],
      "toolEntries": false,
      "valueAbi": 12
    },
    {
      "acceptance": {
        "catalogs": [],
        "functionLimitCountsImports": false,
        "helperAllowance": 13,
        "maxDefinedFunctions": 270,
        "values": [
          12
        ]
      },
      "admittedImports": 58,
      "exceptionChannel": true,
      "exceptionVersion": {
        "global": "varta.exception.version_v3",
        "value": 3
      },
      "fairFuel": false,
      "frames": null,
      "hostAbi": 26,
      "httpVersion": null,
      "name": "checked-f64-utf16-v24",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10
      ],
      "toolEntries": false,
      "valueAbi": 12
    },
    {
      "acceptance": {
        "catalogs": [],
        "functionLimitCountsImports": false,
        "helperAllowance": 13,
        "maxDefinedFunctions": 270,
        "values": [
          12
        ]
      },
      "admittedImports": 58,
      "exceptionChannel": true,
      "exceptionVersion": {
        "global": "varta.exception.version_v3",
        "value": 3
      },
      "fairFuel": false,
      "frames": null,
      "hostAbi": 26,
      "httpVersion": null,
      "name": "checked-f64-utf16-v25",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10
      ],
      "toolEntries": false,
      "valueAbi": 12
    },
    {
      "acceptance": {
        "catalogs": [],
        "functionLimitCountsImports": false,
        "helperAllowance": 13,
        "maxDefinedFunctions": 270,
        "values": [
          12
        ]
      },
      "admittedImports": 58,
      "exceptionChannel": true,
      "exceptionVersion": {
        "global": "varta.exception.version_v3",
        "value": 3
      },
      "fairFuel": false,
      "frames": null,
      "hostAbi": 26,
      "httpVersion": null,
      "name": "checked-f64-utf16-v26",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10
      ],
      "toolEntries": false,
      "valueAbi": 12
    },
    {
      "acceptance": {
        "catalogs": [],
        "functionLimitCountsImports": false,
        "helperAllowance": 13,
        "maxDefinedFunctions": 270,
        "values": [
          12
        ]
      },
      "admittedImports": 58,
      "exceptionChannel": true,
      "exceptionVersion": {
        "global": "varta.exception.version_v3",
        "value": 3
      },
      "fairFuel": false,
      "frames": null,
      "hostAbi": 26,
      "httpVersion": null,
      "name": "checked-f64-utf16-v27",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10
      ],
      "toolEntries": false,
      "valueAbi": 12
    },
    {
      "acceptance": {
        "catalogs": [],
        "functionLimitCountsImports": false,
        "helperAllowance": 13,
        "maxDefinedFunctions": 270,
        "values": [
          12
        ]
      },
      "admittedImports": 68,
      "exceptionChannel": true,
      "exceptionVersion": {
        "global": "varta.exception.version_v3",
        "value": 3
      },
      "fairFuel": false,
      "frames": null,
      "hostAbi": 27,
      "httpVersion": null,
      "name": "checked-f64-utf16-v28",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10
      ],
      "toolEntries": false,
      "valueAbi": 12
    },
    {
      "acceptance": {
        "catalogs": [],
        "functionLimitCountsImports": false,
        "helperAllowance": 13,
        "maxDefinedFunctions": 270,
        "values": [
          13
        ]
      },
      "admittedImports": 75,
      "exceptionChannel": true,
      "exceptionVersion": {
        "global": "varta.exception.version_v4",
        "value": 4
      },
      "fairFuel": false,
      "frames": null,
      "hostAbi": 28,
      "httpVersion": null,
      "name": "checked-f64-utf16-v29",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10,
        11
      ],
      "toolEntries": false,
      "valueAbi": 13
    },
    {
      "acceptance": {
        "catalogs": [
          {
            "frames": false,
            "version": 1
          }
        ],
        "functionLimitCountsImports": false,
        "helperAllowance": 13,
        "maxDefinedFunctions": 270,
        "values": [
          13
        ]
      },
      "admittedImports": 75,
      "exceptionChannel": true,
      "exceptionVersion": {
        "global": "varta.exception.version_v4",
        "value": 4
      },
      "fairFuel": false,
      "frames": null,
      "hostAbi": 28,
      "httpVersion": null,
      "name": "checked-f64-utf16-v30",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10,
        11
      ],
      "toolEntries": false,
      "valueAbi": 13
    },
    {
      "acceptance": {
        "catalogs": [
          {
            "frames": false,
            "version": 1
          }
        ],
        "functionLimitCountsImports": false,
        "helperAllowance": 13,
        "maxDefinedFunctions": 270,
        "values": [
          13
        ]
      },
      "admittedImports": 75,
      "exceptionChannel": true,
      "exceptionVersion": {
        "global": "varta.exception.version_v4",
        "value": 4
      },
      "fairFuel": false,
      "frames": null,
      "hostAbi": 28,
      "httpVersion": null,
      "name": "checked-f64-utf16-v31",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10,
        11
      ],
      "toolEntries": false,
      "valueAbi": 13
    },
    {
      "acceptance": {
        "catalogs": [
          {
            "frames": false,
            "version": 1
          }
        ],
        "functionLimitCountsImports": false,
        "helperAllowance": 13,
        "maxDefinedFunctions": 270,
        "values": [
          13
        ]
      },
      "admittedImports": 75,
      "exceptionChannel": true,
      "exceptionVersion": {
        "global": "varta.exception.version_v5",
        "value": 5
      },
      "fairFuel": false,
      "frames": null,
      "hostAbi": 28,
      "httpVersion": null,
      "name": "checked-f64-utf16-v32",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10,
        11
      ],
      "toolEntries": false,
      "valueAbi": 13
    },
    {
      "acceptance": {
        "catalogs": [
          {
            "frames": false,
            "version": 1
          },
          {
            "frames": true,
            "version": 2
          }
        ],
        "functionLimitCountsImports": false,
        "helperAllowance": 13,
        "maxDefinedFunctions": 270,
        "values": [
          13
        ]
      },
      "admittedImports": 75,
      "exceptionChannel": true,
      "exceptionVersion": {
        "global": "varta.exception.version_v5",
        "value": 5
      },
      "fairFuel": false,
      "frames": {
        "catalog": 2
      },
      "hostAbi": 29,
      "httpVersion": null,
      "name": "checked-f64-utf16-v33",
      "numbers": "binary64",
      "scheduler": false,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10,
        11
      ],
      "toolEntries": false,
      "valueAbi": 13
    },
    {
      "acceptance": {
        "catalogs": [
          {
            "frames": false,
            "version": 1
          },
          {
            "frames": true,
            "version": 2
          }
        ],
        "functionLimitCountsImports": false,
        "helperAllowance": 13,
        "maxDefinedFunctions": 270,
        "values": [
          15
        ]
      },
      "admittedImports": 80,
      "exceptionChannel": true,
      "exceptionVersion": {
        "global": "varta.exception.version_v6",
        "value": 6
      },
      "fairFuel": false,
      "frames": {
        "catalog": 2
      },
      "hostAbi": 30,
      "httpVersion": null,
      "name": "checked-f64-utf16-v34",
      "numbers": "binary64",
      "scheduler": true,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10,
        11,
        13
      ],
      "toolEntries": false,
      "valueAbi": 15
    },
    {
      "acceptance": {
        "catalogs": [
          {
            "frames": false,
            "version": 1
          },
          {
            "frames": true,
            "version": 3
          }
        ],
        "functionLimitCountsImports": false,
        "helperAllowance": 13,
        "maxDefinedFunctions": 270,
        "values": [
          16
        ]
      },
      "admittedImports": 80,
      "exceptionChannel": true,
      "exceptionVersion": {
        "global": "varta.exception.version_v7",
        "value": 7
      },
      "fairFuel": false,
      "frames": {
        "catalog": 3
      },
      "hostAbi": 31,
      "httpVersion": null,
      "name": "checked-f64-utf16-v35",
      "numbers": "binary64",
      "scheduler": true,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10,
        11,
        13,
        14
      ],
      "toolEntries": false,
      "valueAbi": 16
    },
    {
      "acceptance": {
        "catalogs": [
          {
            "frames": false,
            "version": 1
          },
          {
            "frames": true,
            "version": 3
          }
        ],
        "functionLimitCountsImports": false,
        "helperAllowance": 13,
        "maxDefinedFunctions": 270,
        "values": [
          17
        ]
      },
      "admittedImports": 84,
      "exceptionChannel": true,
      "exceptionVersion": {
        "global": "varta.exception.version_v8",
        "value": 8
      },
      "fairFuel": false,
      "frames": {
        "catalog": 3
      },
      "hostAbi": 32,
      "httpVersion": null,
      "name": "checked-f64-utf16-v36",
      "numbers": "binary64",
      "scheduler": true,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10,
        11,
        12,
        13,
        14
      ],
      "toolEntries": false,
      "valueAbi": 17
    },
    {
      "acceptance": {
        "catalogs": [
          {
            "frames": false,
            "version": 1
          },
          {
            "frames": true,
            "version": 3
          }
        ],
        "functionLimitCountsImports": false,
        "helperAllowance": 13,
        "maxDefinedFunctions": 270,
        "values": [
          17
        ]
      },
      "admittedImports": 93,
      "exceptionChannel": true,
      "exceptionVersion": {
        "global": "varta.exception.version_v8",
        "value": 8
      },
      "fairFuel": false,
      "frames": {
        "catalog": 3
      },
      "hostAbi": 33,
      "httpVersion": null,
      "name": "checked-f64-utf16-v37",
      "numbers": "binary64",
      "scheduler": true,
      "schedulerLimits": {
        "held": 64,
        "queueError": "microtask queue exceeds 64 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "execution exceeds 1048576 promise serials",
        "serials": 1048576,
        "timerError": "execution exceeds 32 pending timers",
        "timers": 32
      },
      "simd128": false,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10,
        11,
        12,
        13,
        14
      ],
      "toolEntries": false,
      "valueAbi": 17
    },
    {
      "acceptance": {
        "catalogs": [
          {
            "frames": false,
            "version": 1
          },
          {
            "frames": true,
            "version": 3
          }
        ],
        "functionLimitCountsImports": true,
        "helperAllowance": 13,
        "maxDefinedFunctions": 1000000,
        "values": [
          18
        ]
      },
      "admittedImports": 55,
      "exceptionChannel": true,
      "exceptionVersion": {
        "global": "varta.exception.version_v9",
        "value": 9
      },
      "fairFuel": true,
      "frames": {
        "catalog": 3
      },
      "hostAbi": 34,
      "httpVersion": 2,
      "name": "checked-f64-utf16-v38",
      "numbers": "binary64",
      "scheduler": true,
      "schedulerLimits": {
        "held": 131072,
        "queueError": "microtask queue exceeds 131072 pending continuations",
        "rejectionError": "more than 1048576 unhandled promise rejections are outstanding",
        "rejections": 1048576,
        "serialError": "instance exceeds 9007199254740991 promise serials",
        "serials": 9007199254740991,
        "timerError": "execution exceeds 65536 pending timers",
        "timers": 65536
      },
      "simd128": true,
      "strings": "UTF-16LE",
      "supportedKinds": [
        0,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10,
        11,
        12,
        13,
        14,
        15,
        16
      ],
      "toolEntries": false,
      "valueAbi": 18
    }
  ],
  "scheduler": {
    "errors": {
      "duplicate": "resume token is already scheduled",
      "jobs": "execution exceeds 65536 microtasks",
      "outcome": "scheduled frame reported an invalid outcome",
      "serials": "execution exceeds 1048576 promise serials",
      "unknown": "unknown promise serial"
    },
    "maxJobs": 65536,
    "maxSerials": 1048576,
    "normal": 0,
    "notRetained": "<reason not retained>",
    "outcomes": [
      0,
      1
    ],
    "reasonBytes": 2048,
    "reasonUnits": 256,
    "resumeExport": "varta_resume",
    "retainedReasons": 16,
    "timer": {
      "capability": "time.timer",
      "deadlineTemplate": "execution deadline exceeded: the next timer is due at least {due} ms after the execution started, beyond the {seconds} s deadline",
      "denialText": "<capability denied: time.timer> (timer)",
      "deniedError": "<capability denied: time.timer> (setTimeout and setInterval require an exact time.timer grant)",
      "interval": 1,
      "maximumDelay": 2147483647.0,
      "minimumDelay": 1.0,
      "stateError": "invalid timer operation",
      "unref": 2
    },
    "truncated": " [reason truncated]"
  },
  "schemaVersion": 1,
  "values": {
    "alignment": 8,
    "allocated": 1,
    "bigint": {
      "digitBytes": 4,
      "flag": 2048,
      "maxBytes": 1048576,
      "meta": 8,
      "negative": 2,
      "positive": 1,
      "zero": 0
    },
    "cellBytes": 16,
    "cellPayload": 8,
    "descriptorEntry": 16,
    "descriptorHeader": 8,
    "diagnosticUnits": 256,
    "errorBytes": 16,
    "errorClasses": [
      "Error",
      "TypeError",
      "RangeError",
      "SyntaxError"
    ],
    "errorFlag": 64,
    "errorOwn": 1,
    "errorShift": 1,
    "exceptionRoot": 1114112,
    "exceptionRootBytes": 8,
    "exceptionValue": "varta.exception.value",
    "firstPageEnd": 65536,
    "flagsOffset": 4,
    "headerBytes": 16,
    "integrity": {
      "flags": 57344,
      "version": 18
    },
    "legacyStringBytes": 1048576,
    "linkOffset": 8,
    "maxKeyBytes": 256,
    "optionalFlag": 2147483648,
    "ownedBase": 1179648,
    "pageBytes": 65536,
    "privatePrefix": "#",
    "recordFlag": 4,
    "recordMeta": 8,
    "recordShapes": [
      {
        "bytes": 12,
        "descriptorBytes": 8,
        "order": 12,
        "presence": 8,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 24,
        "descriptorBytes": 24,
        "order": 20,
        "presence": 16,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 36,
        "descriptorBytes": 40,
        "order": 28,
        "presence": 24,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 48,
        "descriptorBytes": 56,
        "order": 36,
        "presence": 32,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 60,
        "descriptorBytes": 72,
        "order": 44,
        "presence": 40,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 72,
        "descriptorBytes": 88,
        "order": 52,
        "presence": 48,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 84,
        "descriptorBytes": 104,
        "order": 60,
        "presence": 56,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 96,
        "descriptorBytes": 120,
        "order": 68,
        "presence": 64,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 108,
        "descriptorBytes": 136,
        "order": 76,
        "presence": 72,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 120,
        "descriptorBytes": 152,
        "order": 84,
        "presence": 80,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 132,
        "descriptorBytes": 168,
        "order": 92,
        "presence": 88,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 144,
        "descriptorBytes": 184,
        "order": 100,
        "presence": 96,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 156,
        "descriptorBytes": 200,
        "order": 108,
        "presence": 104,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 168,
        "descriptorBytes": 216,
        "order": 116,
        "presence": 112,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 180,
        "descriptorBytes": 232,
        "order": 124,
        "presence": 120,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 192,
        "descriptorBytes": 248,
        "order": 132,
        "presence": 128,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 204,
        "descriptorBytes": 264,
        "order": 140,
        "presence": 136,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 216,
        "descriptorBytes": 280,
        "order": 148,
        "presence": 144,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 228,
        "descriptorBytes": 296,
        "order": 156,
        "presence": 152,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 240,
        "descriptorBytes": 312,
        "order": 164,
        "presence": 160,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 252,
        "descriptorBytes": 328,
        "order": 172,
        "presence": 168,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 264,
        "descriptorBytes": 344,
        "order": 180,
        "presence": 176,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 276,
        "descriptorBytes": 360,
        "order": 188,
        "presence": 184,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 288,
        "descriptorBytes": 376,
        "order": 196,
        "presence": 192,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 300,
        "descriptorBytes": 392,
        "order": 204,
        "presence": 200,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 312,
        "descriptorBytes": 408,
        "order": 212,
        "presence": 208,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 324,
        "descriptorBytes": 424,
        "order": 220,
        "presence": 216,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 336,
        "descriptorBytes": 440,
        "order": 228,
        "presence": 224,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 348,
        "descriptorBytes": 456,
        "order": 236,
        "presence": 232,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 360,
        "descriptorBytes": 472,
        "order": 244,
        "presence": 240,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 372,
        "descriptorBytes": 488,
        "order": 252,
        "presence": 248,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 384,
        "descriptorBytes": 504,
        "order": 260,
        "presence": 256,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 396,
        "descriptorBytes": 520,
        "order": 268,
        "presence": 264,
        "wide": false,
        "words": 1
      },
      {
        "bytes": 412,
        "descriptorBytes": 544,
        "order": 280,
        "presence": 272,
        "wide": true,
        "words": 2
      },
      {
        "bytes": 424,
        "descriptorBytes": 560,
        "order": 288,
        "presence": 280,
        "wide": true,
        "words": 2
      },
      {
        "bytes": 436,
        "descriptorBytes": 576,
        "order": 296,
        "presence": 288,
        "wide": true,
        "words": 2
      },
      {
        "bytes": 448,
        "descriptorBytes": 592,
        "order": 304,
        "presence": 296,
        "wide": true,
        "words": 2
      },
      {
        "bytes": 460,
        "descriptorBytes": 608,
        "order": 312,
        "presence": 304,
        "wide": true,
        "words": 2
      },
      {
        "bytes": 472,
        "descriptorBytes": 624,
        "order": 320,
        "presence": 312,
        "wide": true,
        "words": 2
      },
      {
        "bytes": 484,
        "descriptorBytes": 640,
        "order": 328,
        "presence": 320,
        "wide": true,
        "words": 2
      },
      {
        "bytes": 496,
        "descriptorBytes": 656,
        "order": 336,
        "presence": 328,
        "wide": true,
        "words": 2
      },
      {
        "bytes": 508,
        "descriptorBytes": 672,
        "order": 344,
        "presence": 336,
        "wide": true,
        "words": 2
      },
      {
        "bytes": 520,
        "descriptorBytes": 688,
        "order": 352,
        "presence": 344,
        "wide": true,
        "words": 2
      },
      {
        "bytes": 532,
        "descriptorBytes": 704,
        "order": 360,
        "presence": 352,
        "wide": true,
        "words": 2
      },
      {
        "bytes": 544,
        "descriptorBytes": 720,
        "order": 368,
        "presence": 360,
        "wide": true,
        "words": 2
      },
      {
        "bytes": 556,
        "descriptorBytes": 736,
        "order": 376,
        "presence": 368,
        "wide": true,
        "words": 2
      },
      {
        "bytes": 568,
        "descriptorBytes": 752,
        "order": 384,
        "presence": 376,
        "wide": true,
        "words": 2
      },
      {
        "bytes": 580,
        "descriptorBytes": 768,
        "order": 392,
        "presence": 384,
        "wide": true,
        "words": 2
      },
      {
        "bytes": 592,
        "descriptorBytes": 784,
        "order": 400,
        "presence": 392,
        "wide": true,
        "words": 2
      },
      {
        "bytes": 604,
        "descriptorBytes": 800,
        "order": 408,
        "presence": 400,
        "wide": true,
        "words": 2
      },
      {
        "bytes": 616,
        "descriptorBytes": 816,
        "order": 416,
        "presence": 408,
        "wide": true,
        "words": 2
      },
      {
        "bytes": 628,
        "descriptorBytes": 832,
        "order": 424,
        "presence": 416,
        "wide": true,
        "words": 2
      },
      {
        "bytes": 640,
        "descriptorBytes": 848,
        "order": 432,
        "presence": 424,
        "wide": true,
        "words": 2
      },
      {
        "bytes": 652,
        "descriptorBytes": 864,
        "order": 440,
        "presence": 432,
        "wide": true,
        "words": 2
      },
      {
        "bytes": 664,
        "descriptorBytes": 880,
        "order": 448,
        "presence": 440,
        "wide": true,
        "words": 2
      },
      {
        "bytes": 676,
        "descriptorBytes": 896,
        "order": 456,
        "presence": 448,
        "wide": true,
        "words": 2
      },
      {
        "bytes": 688,
        "descriptorBytes": 912,
        "order": 464,
        "presence": 456,
        "wide": true,
        "words": 2
      },
      {
        "bytes": 700,
        "descriptorBytes": 928,
        "order": 472,
        "presence": 464,
        "wide": true,
        "words": 2
      },
      {
        "bytes": 712,
        "descriptorBytes": 944,
        "order": 480,
        "presence": 472,
        "wide": true,
        "words": 2
      },
      {
        "bytes": 724,
        "descriptorBytes": 960,
        "order": 488,
        "presence": 480,
        "wide": true,
        "words": 2
      },
      {
        "bytes": 736,
        "descriptorBytes": 976,
        "order": 496,
        "presence": 488,
        "wide": true,
        "words": 2
      },
      {
        "bytes": 748,
        "descriptorBytes": 992,
        "order": 504,
        "presence": 496,
        "wide": true,
        "words": 2
      },
      {
        "bytes": 760,
        "descriptorBytes": 1008,
        "order": 512,
        "presence": 504,
        "wide": true,
        "words": 2
      },
      {
        "bytes": 772,
        "descriptorBytes": 1024,
        "order": 520,
        "presence": 512,
        "wide": true,
        "words": 2
      },
      {
        "bytes": 784,
        "descriptorBytes": 1040,
        "order": 528,
        "presence": 520,
        "wide": true,
        "words": 2
      },
      {
        "bytes": 800,
        "descriptorBytes": 1060,
        "order": 540,
        "presence": 528,
        "wide": true,
        "words": 3
      },
      {
        "bytes": 812,
        "descriptorBytes": 1076,
        "order": 548,
        "presence": 536,
        "wide": true,
        "words": 3
      },
      {
        "bytes": 824,
        "descriptorBytes": 1092,
        "order": 556,
        "presence": 544,
        "wide": true,
        "words": 3
      },
      {
        "bytes": 836,
        "descriptorBytes": 1108,
        "order": 564,
        "presence": 552,
        "wide": true,
        "words": 3
      },
      {
        "bytes": 848,
        "descriptorBytes": 1124,
        "order": 572,
        "presence": 560,
        "wide": true,
        "words": 3
      },
      {
        "bytes": 860,
        "descriptorBytes": 1140,
        "order": 580,
        "presence": 568,
        "wide": true,
        "words": 3
      },
      {
        "bytes": 872,
        "descriptorBytes": 1156,
        "order": 588,
        "presence": 576,
        "wide": true,
        "words": 3
      },
      {
        "bytes": 884,
        "descriptorBytes": 1172,
        "order": 596,
        "presence": 584,
        "wide": true,
        "words": 3
      },
      {
        "bytes": 896,
        "descriptorBytes": 1188,
        "order": 604,
        "presence": 592,
        "wide": true,
        "words": 3
      },
      {
        "bytes": 908,
        "descriptorBytes": 1204,
        "order": 612,
        "presence": 600,
        "wide": true,
        "words": 3
      },
      {
        "bytes": 920,
        "descriptorBytes": 1220,
        "order": 620,
        "presence": 608,
        "wide": true,
        "words": 3
      },
      {
        "bytes": 932,
        "descriptorBytes": 1236,
        "order": 628,
        "presence": 616,
        "wide": true,
        "words": 3
      },
      {
        "bytes": 944,
        "descriptorBytes": 1252,
        "order": 636,
        "presence": 624,
        "wide": true,
        "words": 3
      },
      {
        "bytes": 956,
        "descriptorBytes": 1268,
        "order": 644,
        "presence": 632,
        "wide": true,
        "words": 3
      },
      {
        "bytes": 968,
        "descriptorBytes": 1284,
        "order": 652,
        "presence": 640,
        "wide": true,
        "words": 3
      },
      {
        "bytes": 980,
        "descriptorBytes": 1300,
        "order": 660,
        "presence": 648,
        "wide": true,
        "words": 3
      },
      {
        "bytes": 992,
        "descriptorBytes": 1316,
        "order": 668,
        "presence": 656,
        "wide": true,
        "words": 3
      },
      {
        "bytes": 1004,
        "descriptorBytes": 1332,
        "order": 676,
        "presence": 664,
        "wide": true,
        "words": 3
      },
      {
        "bytes": 1016,
        "descriptorBytes": 1348,
        "order": 684,
        "presence": 672,
        "wide": true,
        "words": 3
      },
      {
        "bytes": 1028,
        "descriptorBytes": 1364,
        "order": 692,
        "presence": 680,
        "wide": true,
        "words": 3
      },
      {
        "bytes": 1040,
        "descriptorBytes": 1380,
        "order": 700,
        "presence": 688,
        "wide": true,
        "words": 3
      },
      {
        "bytes": 1052,
        "descriptorBytes": 1396,
        "order": 708,
        "presence": 696,
        "wide": true,
        "words": 3
      },
      {
        "bytes": 1064,
        "descriptorBytes": 1412,
        "order": 716,
        "presence": 704,
        "wide": true,
        "words": 3
      },
      {
        "bytes": 1076,
        "descriptorBytes": 1428,
        "order": 724,
        "presence": 712,
        "wide": true,
        "words": 3
      },
      {
        "bytes": 1088,
        "descriptorBytes": 1444,
        "order": 732,
        "presence": 720,
        "wide": true,
        "words": 3
      },
      {
        "bytes": 1100,
        "descriptorBytes": 1460,
        "order": 740,
        "presence": 728,
        "wide": true,
        "words": 3
      },
      {
        "bytes": 1112,
        "descriptorBytes": 1476,
        "order": 748,
        "presence": 736,
        "wide": true,
        "words": 3
      },
      {
        "bytes": 1124,
        "descriptorBytes": 1492,
        "order": 756,
        "presence": 744,
        "wide": true,
        "words": 3
      },
      {
        "bytes": 1136,
        "descriptorBytes": 1508,
        "order": 764,
        "presence": 752,
        "wide": true,
        "words": 3
      },
      {
        "bytes": 1148,
        "descriptorBytes": 1524,
        "order": 772,
        "presence": 760,
        "wide": true,
        "words": 3
      },
      {
        "bytes": 1160,
        "descriptorBytes": 1540,
        "order": 780,
        "presence": 768,
        "wide": true,
        "words": 3
      },
      {
        "bytes": 1172,
        "descriptorBytes": 1556,
        "order": 788,
        "presence": 776,
        "wide": true,
        "words": 3
      },
      {
        "bytes": 1188,
        "descriptorBytes": 1576,
        "order": 800,
        "presence": 784,
        "wide": true,
        "words": 4
      },
      {
        "bytes": 1200,
        "descriptorBytes": 1592,
        "order": 808,
        "presence": 792,
        "wide": true,
        "words": 4
      },
      {
        "bytes": 1212,
        "descriptorBytes": 1608,
        "order": 816,
        "presence": 800,
        "wide": true,
        "words": 4
      },
      {
        "bytes": 1224,
        "descriptorBytes": 1624,
        "order": 824,
        "presence": 808,
        "wide": true,
        "words": 4
      },
      {
        "bytes": 1236,
        "descriptorBytes": 1640,
        "order": 832,
        "presence": 816,
        "wide": true,
        "words": 4
      },
      {
        "bytes": 1248,
        "descriptorBytes": 1656,
        "order": 840,
        "presence": 824,
        "wide": true,
        "words": 4
      },
      {
        "bytes": 1260,
        "descriptorBytes": 1672,
        "order": 848,
        "presence": 832,
        "wide": true,
        "words": 4
      },
      {
        "bytes": 1272,
        "descriptorBytes": 1688,
        "order": 856,
        "presence": 840,
        "wide": true,
        "words": 4
      },
      {
        "bytes": 1284,
        "descriptorBytes": 1704,
        "order": 864,
        "presence": 848,
        "wide": true,
        "words": 4
      },
      {
        "bytes": 1296,
        "descriptorBytes": 1720,
        "order": 872,
        "presence": 856,
        "wide": true,
        "words": 4
      },
      {
        "bytes": 1308,
        "descriptorBytes": 1736,
        "order": 880,
        "presence": 864,
        "wide": true,
        "words": 4
      },
      {
        "bytes": 1320,
        "descriptorBytes": 1752,
        "order": 888,
        "presence": 872,
        "wide": true,
        "words": 4
      },
      {
        "bytes": 1332,
        "descriptorBytes": 1768,
        "order": 896,
        "presence": 880,
        "wide": true,
        "words": 4
      },
      {
        "bytes": 1344,
        "descriptorBytes": 1784,
        "order": 904,
        "presence": 888,
        "wide": true,
        "words": 4
      },
      {
        "bytes": 1356,
        "descriptorBytes": 1800,
        "order": 912,
        "presence": 896,
        "wide": true,
        "words": 4
      },
      {
        "bytes": 1368,
        "descriptorBytes": 1816,
        "order": 920,
        "presence": 904,
        "wide": true,
        "words": 4
      },
      {
        "bytes": 1380,
        "descriptorBytes": 1832,
        "order": 928,
        "presence": 912,
        "wide": true,
        "words": 4
      },
      {
        "bytes": 1392,
        "descriptorBytes": 1848,
        "order": 936,
        "presence": 920,
        "wide": true,
        "words": 4
      },
      {
        "bytes": 1404,
        "descriptorBytes": 1864,
        "order": 944,
        "presence": 928,
        "wide": true,
        "words": 4
      },
      {
        "bytes": 1416,
        "descriptorBytes": 1880,
        "order": 952,
        "presence": 936,
        "wide": true,
        "words": 4
      },
      {
        "bytes": 1428,
        "descriptorBytes": 1896,
        "order": 960,
        "presence": 944,
        "wide": true,
        "words": 4
      },
      {
        "bytes": 1440,
        "descriptorBytes": 1912,
        "order": 968,
        "presence": 952,
        "wide": true,
        "words": 4
      },
      {
        "bytes": 1452,
        "descriptorBytes": 1928,
        "order": 976,
        "presence": 960,
        "wide": true,
        "words": 4
      },
      {
        "bytes": 1464,
        "descriptorBytes": 1944,
        "order": 984,
        "presence": 968,
        "wide": true,
        "words": 4
      },
      {
        "bytes": 1476,
        "descriptorBytes": 1960,
        "order": 992,
        "presence": 976,
        "wide": true,
        "words": 4
      },
      {
        "bytes": 1488,
        "descriptorBytes": 1976,
        "order": 1000,
        "presence": 984,
        "wide": true,
        "words": 4
      },
      {
        "bytes": 1500,
        "descriptorBytes": 1992,
        "order": 1008,
        "presence": 992,
        "wide": true,
        "words": 4
      },
      {
        "bytes": 1512,
        "descriptorBytes": 2008,
        "order": 1016,
        "presence": 1000,
        "wide": true,
        "words": 4
      },
      {
        "bytes": 1524,
        "descriptorBytes": 2024,
        "order": 1024,
        "presence": 1008,
        "wide": true,
        "words": 4
      },
      {
        "bytes": 1536,
        "descriptorBytes": 2040,
        "order": 1032,
        "presence": 1016,
        "wide": true,
        "words": 4
      },
      {
        "bytes": 1548,
        "descriptorBytes": 2056,
        "order": 1040,
        "presence": 1024,
        "wide": true,
        "words": 4
      },
      {
        "bytes": 1560,
        "descriptorBytes": 2072,
        "order": 1048,
        "presence": 1032,
        "wide": true,
        "words": 4
      }
    ],
    "references": [
      3,
      4,
      5,
      8,
      9,
      10,
      11,
      12,
      13,
      14,
      15,
      16
    ],
    "slotBytes": 8,
    "staticSection": "varta.static",
    "tags": {
      "Array": 5,
      "BigInt": 11,
      "Boolean": 2,
      "Bytes": 15,
      "Dictionary": 13,
      "Error": 10,
      "Float64": 6,
      "Function": 12,
      "I32": 1,
      "Null": 0,
      "PrimitiveUnion": 8,
      "Promise": 14,
      "Record": 4,
      "ReferenceUnion": 9,
      "String": 3,
      "Symbol": 16,
      "Undefined": 7
    },
    "unionFlag": 16
  },
  "view": {
    "accessibility": {
      "implicitRoles": {
        "a": "link",
        "aside": "complementary",
        "button": "button",
        "dialog": "dialog",
        "footer": "contentinfo",
        "form": "form",
        "h1": "heading",
        "h2": "heading",
        "h3": "heading",
        "header": "banner",
        "img": "image",
        "input": "textbox",
        "li": "listitem",
        "main": "main",
        "nav": "navigation",
        "ol": "list",
        "option": "option",
        "p": "paragraph",
        "progress": "progressbar",
        "select": "combobox",
        "textarea": "textbox",
        "ul": "list"
      },
      "inputRoles": {
        "checkbox": "checkbox",
        "number": "spinbutton",
        "password": "textbox",
        "radio": "radio",
        "search": "searchbox",
        "text": "textbox"
      },
      "labelable": [
        "input",
        "textarea",
        "select",
        "progress"
      ],
      "nameFromContent": [
        "button",
        "link",
        "heading",
        "option",
        "cell",
        "columnheader",
        "row",
        "tab",
        "checkbox",
        "radio"
      ]
    },
    "attributes": [
      {
        "code": 1,
        "grammar": {
          "kind": "ident",
          "values": null
        },
        "name": "id",
        "tags": "all"
      },
      {
        "code": 2,
        "grammar": {
          "kind": "elementType",
          "values": null
        },
        "name": "type",
        "tags": [
          "input",
          "button"
        ]
      },
      {
        "code": 3,
        "grammar": {
          "kind": "text",
          "values": null
        },
        "name": "name",
        "tags": [
          "input",
          "select",
          "textarea"
        ]
      },
      {
        "code": 4,
        "grammar": {
          "kind": "text",
          "values": null
        },
        "name": "value",
        "tags": [
          "option",
          "progress"
        ]
      },
      {
        "code": 5,
        "grammar": {
          "kind": "text",
          "values": null
        },
        "name": "placeholder",
        "tags": [
          "input",
          "textarea"
        ]
      },
      {
        "code": 6,
        "grammar": {
          "kind": "boolean",
          "values": null
        },
        "name": "disabled",
        "tags": [
          "button",
          "input",
          "select",
          "textarea",
          "option"
        ]
      },
      {
        "code": 7,
        "grammar": {
          "kind": "boolean",
          "values": null
        },
        "name": "readonly",
        "tags": [
          "input",
          "textarea"
        ]
      },
      {
        "code": 8,
        "grammar": {
          "kind": "count",
          "values": null
        },
        "name": "maxlength",
        "tags": [
          "input",
          "textarea"
        ]
      },
      {
        "code": 9,
        "grammar": {
          "kind": "boolean",
          "values": null
        },
        "name": "autofocus",
        "tags": [
          "button",
          "input",
          "select",
          "textarea"
        ]
      },
      {
        "code": 10,
        "grammar": {
          "kind": "boolean",
          "values": null
        },
        "name": "selected",
        "tags": [
          "option"
        ]
      },
      {
        "code": 11,
        "grammar": {
          "kind": "ident",
          "values": null
        },
        "name": "for",
        "tags": [
          "label"
        ]
      },
      {
        "code": 12,
        "grammar": {
          "kind": "href",
          "values": null
        },
        "name": "href",
        "tags": [
          "a"
        ]
      },
      {
        "code": 13,
        "grammar": {
          "kind": "resource",
          "values": null
        },
        "name": "src",
        "tags": [
          "img"
        ]
      },
      {
        "code": 14,
        "grammar": {
          "kind": "text",
          "values": null
        },
        "name": "alt",
        "tags": [
          "img"
        ]
      },
      {
        "code": 15,
        "grammar": {
          "kind": "keywords",
          "values": [
            "0",
            "-1"
          ]
        },
        "name": "tabindex",
        "tags": "all"
      },
      {
        "code": 16,
        "grammar": {
          "kind": "number",
          "values": null
        },
        "name": "max",
        "tags": [
          "progress"
        ]
      },
      {
        "code": 17,
        "grammar": {
          "kind": "keywords",
          "values": [
            "table",
            "row",
            "cell",
            "columnheader",
            "dialog",
            "alert",
            "status",
            "navigation",
            "list",
            "listitem",
            "tab",
            "tablist",
            "tabpanel",
            "complementary"
          ]
        },
        "name": "role",
        "tags": "all"
      },
      {
        "code": 18,
        "grammar": {
          "kind": "text",
          "values": null
        },
        "name": "aria-label",
        "tags": "all"
      },
      {
        "code": 19,
        "grammar": {
          "kind": "identList",
          "values": null
        },
        "name": "aria-labelledby",
        "tags": "all"
      },
      {
        "code": 20,
        "grammar": {
          "kind": "identList",
          "values": null
        },
        "name": "aria-describedby",
        "tags": "all"
      },
      {
        "code": 21,
        "grammar": {
          "kind": "keywords",
          "values": [
            "off",
            "polite",
            "assertive"
          ]
        },
        "name": "aria-live",
        "tags": "all"
      },
      {
        "code": 22,
        "grammar": {
          "kind": "keywords",
          "values": [
            "true",
            "false"
          ]
        },
        "name": "aria-expanded",
        "tags": "all"
      },
      {
        "code": 23,
        "grammar": {
          "kind": "keywords",
          "values": [
            "true",
            "false"
          ]
        },
        "name": "aria-selected",
        "tags": "all"
      },
      {
        "code": 24,
        "grammar": {
          "kind": "keywords",
          "values": [
            "page",
            "step",
            "location",
            "date",
            "time",
            "true",
            "false"
          ]
        },
        "name": "aria-current",
        "tags": "all"
      },
      {
        "code": 25,
        "grammar": {
          "kind": "keywords",
          "values": [
            "true",
            "false"
          ]
        },
        "name": "aria-busy",
        "tags": "all"
      },
      {
        "code": 26,
        "grammar": {
          "kind": "keywords",
          "values": [
            "true",
            "false"
          ]
        },
        "name": "aria-invalid",
        "tags": "all"
      },
      {
        "code": 27,
        "grammar": {
          "kind": "keywords",
          "values": [
            "true",
            "false"
          ]
        },
        "name": "aria-hidden",
        "tags": "all"
      },
      {
        "code": 28,
        "grammar": {
          "kind": "positiveCount",
          "values": null
        },
        "name": "rows",
        "tags": [
          "textarea"
        ]
      },
      {
        "code": 29,
        "grammar": {
          "kind": "count",
          "values": null
        },
        "name": "width",
        "tags": [
          "img"
        ]
      },
      {
        "code": 30,
        "grammar": {
          "kind": "count",
          "values": null
        },
        "name": "height",
        "tags": [
          "img"
        ]
      },
      {
        "code": 31,
        "grammar": {
          "kind": "imageCandidates",
          "values": null
        },
        "name": "srcset",
        "tags": [
          "img"
        ]
      },
      {
        "code": 32,
        "grammar": {
          "kind": "sourceSizes",
          "values": null
        },
        "name": "sizes",
        "tags": [
          "img"
        ]
      },
      {
        "code": 33,
        "grammar": {
          "kind": "keywords",
          "values": [
            "sync",
            "async",
            "auto"
          ]
        },
        "name": "decoding",
        "tags": [
          "img"
        ]
      },
      {
        "code": 34,
        "grammar": {
          "kind": "keywords",
          "values": [
            "eager",
            "lazy"
          ]
        },
        "name": "loading",
        "tags": [
          "img"
        ]
      }
    ],
    "buttonTypes": [
      "button",
      "submit"
    ],
    "capabilities": {
      "anyHttps": "https://*",
      "clipboard": "clipboard.write",
      "openUrl": "ui.open_url"
    },
    "commands": [
      {
        "argument": false,
        "code": 1,
        "name": "focus",
        "tags": "focusable"
      },
      {
        "argument": false,
        "code": 2,
        "name": "blur",
        "tags": "focusable"
      },
      {
        "argument": false,
        "code": 3,
        "name": "select",
        "tags": [
          "input",
          "textarea"
        ]
      },
      {
        "argument": false,
        "code": 4,
        "name": "scroll_into_view",
        "tags": "all"
      },
      {
        "argument": true,
        "code": 5,
        "name": "scroll_to",
        "tags": "all"
      },
      {
        "argument": false,
        "code": 6,
        "name": "show_modal",
        "tags": [
          "dialog"
        ]
      },
      {
        "argument": false,
        "code": 7,
        "name": "close",
        "tags": [
          "dialog"
        ]
      }
    ],
    "css": [
      {
        "code": 1,
        "grammar": "block | flex | grid | none | inline",
        "name": "display",
        "note": null
      },
      {
        "code": 2,
        "grammar": "auto | <length-percentage>",
        "name": "width",
        "note": null
      },
      {
        "code": 3,
        "grammar": "auto | <length-percentage>",
        "name": "height",
        "note": null
      },
      {
        "code": 4,
        "grammar": "auto | <length-percentage>",
        "name": "min-width",
        "note": null
      },
      {
        "code": 5,
        "grammar": "auto | <length-percentage>",
        "name": "min-height",
        "note": null
      },
      {
        "code": 6,
        "grammar": "none | <length-percentage>",
        "name": "max-width",
        "note": null
      },
      {
        "code": 7,
        "grammar": "none | <length-percentage>",
        "name": "max-height",
        "note": null
      },
      {
        "code": 8,
        "grammar": "[auto | <length-percentage>]{1,4}",
        "name": "margin",
        "note": null
      },
      {
        "code": 9,
        "grammar": "auto | <length-percentage>",
        "name": "margin-top",
        "note": null
      },
      {
        "code": 10,
        "grammar": "auto | <length-percentage>",
        "name": "margin-right",
        "note": null
      },
      {
        "code": 11,
        "grammar": "auto | <length-percentage>",
        "name": "margin-bottom",
        "note": null
      },
      {
        "code": 12,
        "grammar": "auto | <length-percentage>",
        "name": "margin-left",
        "note": null
      },
      {
        "code": 13,
        "grammar": "<length-percentage>{1,4}",
        "name": "padding",
        "note": "nonnegative"
      },
      {
        "code": 14,
        "grammar": "<length-percentage>",
        "name": "padding-top",
        "note": "nonnegative"
      },
      {
        "code": 15,
        "grammar": "<length-percentage>",
        "name": "padding-right",
        "note": "nonnegative"
      },
      {
        "code": 16,
        "grammar": "<length-percentage>",
        "name": "padding-bottom",
        "note": "nonnegative"
      },
      {
        "code": 17,
        "grammar": "<length-percentage>",
        "name": "padding-left",
        "note": "nonnegative"
      },
      {
        "code": 18,
        "grammar": "<length-percentage>{1,2}",
        "name": "gap",
        "note": null
      },
      {
        "code": 19,
        "grammar": "<length-percentage>",
        "name": "row-gap",
        "note": null
      },
      {
        "code": 20,
        "grammar": "<length-percentage>",
        "name": "column-gap",
        "note": null
      },
      {
        "code": 21,
        "grammar": "row | row-reverse | column | column-reverse",
        "name": "flex-direction",
        "note": null
      },
      {
        "code": 22,
        "grammar": "nowrap | wrap | wrap-reverse",
        "name": "flex-wrap",
        "note": null
      },
      {
        "code": 23,
        "grammar": "start | end | flex-start | flex-end | center | space-between | space-around | space-evenly",
        "name": "justify-content",
        "note": null
      },
      {
        "code": 24,
        "grammar": "stretch | start | end | flex-start | flex-end | center | baseline",
        "name": "align-items",
        "note": null
      },
      {
        "code": 25,
        "grammar": "auto | stretch | start | end | flex-start | flex-end | center | baseline",
        "name": "align-self",
        "note": null
      },
      {
        "code": 26,
        "grammar": "<number>",
        "name": "flex-grow",
        "note": "nonnegative"
      },
      {
        "code": 27,
        "grammar": "<number>",
        "name": "flex-shrink",
        "note": "nonnegative"
      },
      {
        "code": 28,
        "grammar": "auto | <length-percentage>",
        "name": "flex-basis",
        "note": null
      },
      {
        "code": 29,
        "grammar": "none | <track>+",
        "name": "grid-template-columns",
        "note": null
      },
      {
        "code": 30,
        "grammar": "none | <track>+",
        "name": "grid-template-rows",
        "note": null
      },
      {
        "code": 31,
        "grammar": "<line> [/ <line>]?",
        "name": "grid-column",
        "note": null
      },
      {
        "code": 32,
        "grammar": "<line> [/ <line>]?",
        "name": "grid-row",
        "note": null
      },
      {
        "code": 33,
        "grammar": "static | relative | absolute | fixed",
        "name": "position",
        "note": null
      },
      {
        "code": 34,
        "grammar": "auto | <length-percentage>",
        "name": "top",
        "note": null
      },
      {
        "code": 35,
        "grammar": "auto | <length-percentage>",
        "name": "right",
        "note": null
      },
      {
        "code": 36,
        "grammar": "auto | <length-percentage>",
        "name": "bottom",
        "note": null
      },
      {
        "code": 37,
        "grammar": "auto | <length-percentage>",
        "name": "left",
        "note": null
      },
      {
        "code": 38,
        "grammar": "auto | <integer>",
        "name": "z-index",
        "note": null
      },
      {
        "code": 39,
        "grammar": "[visible | hidden | clip | scroll | auto]{1,2}",
        "name": "overflow",
        "note": null
      },
      {
        "code": 40,
        "grammar": "<length> || [none | hidden | solid] || <color>",
        "name": "border",
        "note": null
      },
      {
        "code": 41,
        "grammar": "<length>{1,4}",
        "name": "border-width",
        "note": null
      },
      {
        "code": 42,
        "grammar": "[none | hidden | solid]{1,4}",
        "name": "border-style",
        "note": null
      },
      {
        "code": 43,
        "grammar": "<color>{1,4}",
        "name": "border-color",
        "note": null
      },
      {
        "code": 44,
        "grammar": "<length-percentage>{1,4}",
        "name": "border-radius",
        "note": null
      },
      {
        "code": 45,
        "grammar": "<color>",
        "name": "background-color",
        "note": null
      },
      {
        "code": 46,
        "grammar": "<color>",
        "name": "color",
        "note": null
      },
      {
        "code": 47,
        "grammar": "<number>",
        "name": "opacity",
        "note": "0 to 1"
      },
      {
        "code": 48,
        "grammar": "none | [<length> || solid || <color>]",
        "name": "outline",
        "note": null
      },
      {
        "code": 49,
        "grammar": "<length>",
        "name": "outline-width",
        "note": null
      },
      {
        "code": 50,
        "grammar": "none | solid",
        "name": "outline-style",
        "note": null
      },
      {
        "code": 51,
        "grammar": "<color>",
        "name": "outline-color",
        "note": null
      },
      {
        "code": 52,
        "grammar": "<length>",
        "name": "outline-offset",
        "note": null
      },
      {
        "code": 53,
        "grammar": "[<string> | <ident> | sans-serif | serif | monospace | system-ui]#",
        "name": "font-family",
        "note": "artifact fonts or generic families; no CSS-wide keywords"
      },
      {
        "code": 54,
        "grammar": "<length-percentage>",
        "name": "font-size",
        "note": null
      },
      {
        "code": 55,
        "grammar": "normal | bold | 400 | 500 | 600 | 700",
        "name": "font-weight",
        "note": null
      },
      {
        "code": 56,
        "grammar": "normal | italic",
        "name": "font-style",
        "note": null
      },
      {
        "code": 57,
        "grammar": "normal | <number> | <length-percentage>",
        "name": "line-height",
        "note": null
      },
      {
        "code": 58,
        "grammar": "left | center | right",
        "name": "text-align",
        "note": null
      },
      {
        "code": 59,
        "grammar": "none | underline",
        "name": "text-decoration",
        "note": null
      },
      {
        "code": 60,
        "grammar": "normal | nowrap | pre-wrap",
        "name": "white-space",
        "note": null
      },
      {
        "code": 61,
        "grammar": "clip | ellipsis",
        "name": "text-overflow",
        "note": null
      },
      {
        "code": 62,
        "grammar": "normal | anywhere",
        "name": "overflow-wrap",
        "note": null
      },
      {
        "code": 63,
        "grammar": "default | pointer | text | not-allowed",
        "name": "cursor",
        "note": null
      },
      {
        "code": 64,
        "grammar": "none | [translate(<length-percentage> [, <length-percentage>]?) | translateX(<length-percentage>) | translateY(<length-percentage>) | scale(<number> [, <number>]?)]+",
        "name": "transform",
        "note": null
      },
      {
        "code": 65,
        "grammar": "none | all | [opacity | color | background-color | transform]#",
        "name": "transition-property",
        "note": null
      },
      {
        "code": 66,
        "grammar": "<time>#",
        "name": "transition-duration",
        "note": null
      },
      {
        "code": 67,
        "grammar": "[linear | ease | ease-in | ease-out | ease-in-out]#",
        "name": "transition-timing-function",
        "note": null
      },
      {
        "code": 68,
        "grammar": "<time>#",
        "name": "transition-delay",
        "note": null
      },
      {
        "code": 69,
        "grammar": "fixed | content",
        "name": "field-sizing",
        "note": "textarea only"
      },
      {
        "code": 70,
        "grammar": "<length> || [none | hidden | solid] || <color>",
        "name": "border-top",
        "note": null
      },
      {
        "code": 71,
        "grammar": "<length> || [none | hidden | solid] || <color>",
        "name": "border-right",
        "note": null
      },
      {
        "code": 72,
        "grammar": "<length> || [none | hidden | solid] || <color>",
        "name": "border-bottom",
        "note": null
      },
      {
        "code": 73,
        "grammar": "<length> || [none | hidden | solid] || <color>",
        "name": "border-left",
        "note": null
      },
      {
        "code": 74,
        "grammar": "none | vertical",
        "name": "resize",
        "note": "textarea only; treated as `none` with `field-sizing: content`"
      },
      {
        "code": 75,
        "grammar": "fill | contain | cover | none | scale-down",
        "name": "object-fit",
        "note": "replaced elements (`img`)"
      },
      {
        "code": 76,
        "grammar": "[left | center | right | <length-percentage>] [top | center | bottom | <length-percentage>]?",
        "name": "object-position",
        "note": "replaced elements (`img`)"
      },
      {
        "code": 77,
        "grammar": "auto | smooth | high-quality | pixelated | crisp-edges",
        "name": "image-rendering",
        "note": "replaced elements (`img`)"
      },
      {
        "code": 78,
        "grammar": "auto || [<number> [/ <number>]?]",
        "name": "aspect-ratio",
        "note": "positive ratio terms"
      },
      {
        "code": 79,
        "grammar": "baseline | sub | super | text-top | text-bottom | middle | top | bottom | <length-percentage>",
        "name": "vertical-align",
        "note": "inline-level boxes"
      },
      {
        "code": 80,
        "grammar": "from-image | none",
        "name": "image-orientation",
        "note": "inherited"
      },
      {
        "code": 81,
        "grammar": "none | <length-percentage> <length-percentage>?",
        "name": "translate",
        "note": null
      },
      {
        "code": 82,
        "grammar": "none | <number> <number>?",
        "name": "scale",
        "note": "positive factors"
      },
      {
        "code": 83,
        "grammar": "<length-percentage> <length-percentage>",
        "name": "transform-origin",
        "note": null
      },
      {
        "code": 84,
        "grammar": "normal | <length>",
        "name": "letter-spacing",
        "note": "inherited"
      },
      {
        "code": 85,
        "grammar": "<length>",
        "name": "border-top-width",
        "note": "nonnegative"
      },
      {
        "code": 86,
        "grammar": "<length>",
        "name": "border-right-width",
        "note": "nonnegative"
      },
      {
        "code": 87,
        "grammar": "<length>",
        "name": "border-bottom-width",
        "note": "nonnegative"
      },
      {
        "code": 88,
        "grammar": "<length>",
        "name": "border-left-width",
        "note": "nonnegative"
      },
      {
        "code": 89,
        "grammar": "none | hidden | solid",
        "name": "border-top-style",
        "note": null
      },
      {
        "code": 90,
        "grammar": "none | hidden | solid",
        "name": "border-right-style",
        "note": null
      },
      {
        "code": 91,
        "grammar": "none | hidden | solid",
        "name": "border-bottom-style",
        "note": null
      },
      {
        "code": 92,
        "grammar": "none | hidden | solid",
        "name": "border-left-style",
        "note": null
      },
      {
        "code": 93,
        "grammar": "<color>",
        "name": "border-top-color",
        "note": null
      },
      {
        "code": 94,
        "grammar": "<color>",
        "name": "border-right-color",
        "note": null
      },
      {
        "code": 95,
        "grammar": "<color>",
        "name": "border-bottom-color",
        "note": null
      },
      {
        "code": 96,
        "grammar": "<color>",
        "name": "border-left-color",
        "note": null
      },
      {
        "code": 97,
        "grammar": "<length-percentage>{1,2}",
        "name": "border-top-left-radius",
        "note": "nonnegative; horizontal then vertical radius"
      },
      {
        "code": 98,
        "grammar": "<length-percentage>{1,2}",
        "name": "border-top-right-radius",
        "note": "nonnegative; horizontal then vertical radius"
      },
      {
        "code": 99,
        "grammar": "<length-percentage>{1,2}",
        "name": "border-bottom-right-radius",
        "note": "nonnegative; horizontal then vertical radius"
      },
      {
        "code": 100,
        "grammar": "<length-percentage>{1,2}",
        "name": "border-bottom-left-radius",
        "note": "nonnegative; horizontal then vertical radius"
      },
      {
        "code": 101,
        "grammar": "visible | hidden | clip | scroll | auto",
        "name": "overflow-x",
        "note": null
      },
      {
        "code": 102,
        "grammar": "visible | hidden | clip | scroll | auto",
        "name": "overflow-y",
        "note": null
      },
      {
        "code": 103,
        "grammar": "visible | hidden | collapse",
        "name": "visibility",
        "note": "inherited"
      },
      {
        "code": 104,
        "grammar": "none | [capitalize | uppercase | lowercase] || full-width || full-size-kana",
        "name": "text-transform",
        "note": "inherited"
      },
      {
        "code": 105,
        "grammar": "auto | none",
        "name": "pointer-events",
        "note": "inherited"
      },
      {
        "code": 106,
        "grammar": "auto | text | none | contain | all",
        "name": "user-select",
        "note": null
      },
      {
        "code": 107,
        "grammar": "content-box | border-box",
        "name": "box-sizing",
        "note": null
      }
    ],
    "cssTypes": {
      "<color>": "<hex> | transparent | currentcolor | rgb(<integer>, <integer>, <integer>) | rgba(<integer>, <integer>, <integer>, <number>)",
      "<length-percentage>": "<length> | <percentage>",
      "<line>": "auto | <integer> | span <integer>",
      "<track>": "<length-percentage> | <flex> | auto | minmax(<track>, <track>) | repeat(<integer>, <track>+)"
    },
    "defaults": {
      "controlFontTags": [
        "button",
        "input",
        "textarea",
        "select"
      ],
      "dialogBackdrop": [
        "rgba(0, 0, 0, 0.4)",
        "rgba(0, 0, 0, 0.6)"
      ],
      "dialogBackground": [
        "#ffffff",
        "#1f1f1f"
      ],
      "dialogText": [
        "#111111",
        "#eeeeee"
      ],
      "monospaceTags": [
        "pre",
        "code"
      ],
      "rootSelector": "[data-l0-root]",
      "rootStyle": "display: block; height: 100%"
    },
    "events": [
      {
        "coalescible": false,
        "code": 1,
        "edits": false,
        "gesture": true,
        "name": "click"
      },
      {
        "coalescible": true,
        "code": 2,
        "edits": true,
        "gesture": false,
        "name": "input"
      },
      {
        "coalescible": false,
        "code": 3,
        "edits": false,
        "gesture": false,
        "name": "change"
      },
      {
        "coalescible": false,
        "code": 4,
        "edits": false,
        "gesture": true,
        "name": "submit"
      },
      {
        "coalescible": false,
        "code": 5,
        "edits": false,
        "gesture": true,
        "name": "keydown"
      },
      {
        "coalescible": false,
        "code": 6,
        "edits": false,
        "gesture": false,
        "name": "focusin"
      },
      {
        "coalescible": false,
        "code": 7,
        "edits": false,
        "gesture": false,
        "name": "focusout"
      },
      {
        "coalescible": true,
        "code": 8,
        "edits": false,
        "gesture": false,
        "name": "scroll"
      },
      {
        "coalescible": false,
        "code": 9,
        "edits": false,
        "gesture": false,
        "name": "cancel"
      },
      {
        "coalescible": false,
        "code": 10,
        "edits": false,
        "gesture": false,
        "name": "close"
      },
      {
        "coalescible": false,
        "code": 11,
        "edits": false,
        "gesture": false,
        "name": "command_result"
      }
    ],
    "filters": {
      "anyKey": 0,
      "flags": {
        "altKey": 4,
        "checked": 32,
        "ctrlKey": 2,
        "isComposing": 16,
        "metaKey": 8,
        "shiftKey": 1
      },
      "modifierMask": 15,
      "options": {
        "notComposing": 4,
        "preventDefault": 1,
        "stopPropagation": 2
      }
    },
    "inputTypes": [
      "text",
      "password",
      "number",
      "search",
      "checkbox",
      "radio"
    ],
    "keys": [
      {
        "code": 1,
        "domKey": "Enter",
        "name": "Enter"
      },
      {
        "code": 2,
        "domKey": "Escape",
        "name": "Escape"
      },
      {
        "code": 3,
        "domKey": "Tab",
        "name": "Tab"
      },
      {
        "code": 4,
        "domKey": " ",
        "name": "Space"
      },
      {
        "code": 5,
        "domKey": "Backspace",
        "name": "Backspace"
      },
      {
        "code": 6,
        "domKey": "Delete",
        "name": "Delete"
      },
      {
        "code": 7,
        "domKey": "ArrowUp",
        "name": "ArrowUp"
      },
      {
        "code": 8,
        "domKey": "ArrowDown",
        "name": "ArrowDown"
      },
      {
        "code": 9,
        "domKey": "ArrowLeft",
        "name": "ArrowLeft"
      },
      {
        "code": 10,
        "domKey": "ArrowRight",
        "name": "ArrowRight"
      },
      {
        "code": 11,
        "domKey": "Home",
        "name": "Home"
      },
      {
        "code": 12,
        "domKey": "End",
        "name": "End"
      },
      {
        "code": 13,
        "domKey": "PageUp",
        "name": "PageUp"
      },
      {
        "code": 14,
        "domKey": "PageDown",
        "name": "PageDown"
      }
    ],
    "limits": {
      "batchBytes": 262144,
      "cascadeWork": 8388608,
      "commands": 16,
      "cssTokens": 128,
      "events": 64,
      "expandedValueBytes": 8388608,
      "fieldValueReservationBytes": 98304,
      "hexDigits": [
        3,
        6,
        8
      ],
      "identChars": 64,
      "listeners": 8192,
      "namingNodeWork": 16,
      "namingWork": 8388608,
      "nodes": 65536,
      "openedUrlBytes": 1048576,
      "openedUrls": 64,
      "operations": 16384,
      "radioNodeWork": 16,
      "radioWork": 8388608,
      "retainedTextBytes": 8388608,
      "selectNodeWork": 16,
      "selectWork": 8388608,
      "snapshotJsonBytes": 16777216,
      "stylesheetBytes": 262144,
      "stylesheetNodeWork": 16,
      "stylesheetWork": 8388608,
      "treeDepth": 512,
      "valueBytes": 65536
    },
    "opcodes": [
      {
        "code": 1,
        "name": "create_element",
        "operands": [
          {
            "kind": "id",
            "name": "node"
          },
          {
            "kind": "code",
            "name": "tag"
          }
        ]
      },
      {
        "code": 2,
        "name": "create_text",
        "operands": [
          {
            "kind": "id",
            "name": "node"
          },
          {
            "kind": "string",
            "name": "text"
          }
        ]
      },
      {
        "code": 3,
        "name": "set_text",
        "operands": [
          {
            "kind": "id",
            "name": "node"
          },
          {
            "kind": "string",
            "name": "text"
          }
        ]
      },
      {
        "code": 4,
        "name": "set_attribute",
        "operands": [
          {
            "kind": "id",
            "name": "node"
          },
          {
            "kind": "code",
            "name": "attribute"
          },
          {
            "kind": "string",
            "name": "text"
          }
        ]
      },
      {
        "code": 5,
        "name": "remove_attribute",
        "operands": [
          {
            "kind": "id",
            "name": "node"
          },
          {
            "kind": "code",
            "name": "attribute"
          }
        ]
      },
      {
        "code": 6,
        "name": "set_class",
        "operands": [
          {
            "kind": "id",
            "name": "node"
          },
          {
            "kind": "string",
            "name": "text"
          }
        ]
      },
      {
        "code": 7,
        "name": "set_style",
        "operands": [
          {
            "kind": "id",
            "name": "node"
          },
          {
            "kind": "code",
            "name": "property"
          },
          {
            "kind": "string",
            "name": "text"
          }
        ]
      },
      {
        "code": 8,
        "name": "remove_style",
        "operands": [
          {
            "kind": "id",
            "name": "node"
          },
          {
            "kind": "code",
            "name": "property"
          }
        ]
      },
      {
        "code": 9,
        "name": "insert",
        "operands": [
          {
            "kind": "id",
            "name": "parent"
          },
          {
            "kind": "id",
            "name": "node"
          },
          {
            "kind": "id",
            "name": "before"
          }
        ]
      },
      {
        "code": 10,
        "name": "remove",
        "operands": [
          {
            "kind": "id",
            "name": "node"
          }
        ]
      },
      {
        "code": 11,
        "name": "listen",
        "operands": [
          {
            "kind": "id",
            "name": "node"
          },
          {
            "kind": "code",
            "name": "event"
          },
          {
            "kind": "id",
            "name": "handler"
          },
          {
            "kind": "u32",
            "name": "options"
          },
          {
            "kind": "code",
            "name": "key"
          },
          {
            "kind": "u32",
            "name": "required_modifiers"
          },
          {
            "kind": "u32",
            "name": "forbidden_modifiers"
          }
        ]
      },
      {
        "code": 12,
        "name": "unlisten",
        "operands": [
          {
            "kind": "id",
            "name": "node"
          },
          {
            "kind": "code",
            "name": "event"
          },
          {
            "kind": "id",
            "name": "handler"
          }
        ]
      },
      {
        "code": 13,
        "name": "set_value",
        "operands": [
          {
            "kind": "id",
            "name": "node"
          },
          {
            "kind": "string",
            "name": "text"
          },
          {
            "kind": "u32",
            "name": "base_seq"
          }
        ]
      },
      {
        "code": 14,
        "name": "set_checked",
        "operands": [
          {
            "kind": "id",
            "name": "node"
          },
          {
            "kind": "u32",
            "name": "checked"
          },
          {
            "kind": "u32",
            "name": "base_seq"
          }
        ]
      },
      {
        "code": 15,
        "name": "command",
        "operands": [
          {
            "kind": "id",
            "name": "node"
          },
          {
            "kind": "code",
            "name": "command"
          },
          {
            "kind": "f64",
            "name": "argument"
          }
        ]
      },
      {
        "code": 16,
        "name": "stylesheet",
        "operands": [
          {
            "kind": "id",
            "name": "sheet"
          },
          {
            "kind": "string",
            "name": "text"
          }
        ]
      },
      {
        "code": 17,
        "name": "set_title",
        "operands": [
          {
            "kind": "string",
            "name": "text"
          }
        ]
      },
      {
        "code": 18,
        "name": "copy_text",
        "operands": [
          {
            "kind": "id",
            "name": "request"
          },
          {
            "kind": "string",
            "name": "text"
          }
        ]
      }
    ],
    "statuses": {
      "invalid_context": 3,
      "not_allowed": 1,
      "ok": 0,
      "refused_by_platform": 2
    },
    "stylesheet": {
      "forbidden": [
        "\\",
        "<",
        ">",
        "/*",
        "@import",
        "!important",
        "url(",
        "image-set(",
        "expression("
      ],
      "forbiddenInline": [
        ";",
        "{",
        "}"
      ],
      "media": [
        "prefers-color-scheme",
        "min-width",
        "max-width"
      ],
      "pseudo": [
        ":hover",
        ":active",
        ":focus",
        ":focus-within",
        ":focus-visible",
        ":disabled",
        ":enabled",
        ":checked",
        ":root",
        ":host",
        ":scope",
        ":first-child",
        ":last-child",
        ":only-child",
        ":first-of-type",
        ":last-of-type",
        ":only-of-type",
        ":nth-child()",
        ":nth-last-child()",
        ":nth-of-type()",
        ":nth-last-of-type()",
        ":empty",
        ":link",
        ":any-link",
        ":visited",
        ":target",
        ":target-within",
        ":read-only",
        ":read-write",
        ":placeholder-shown",
        ":default",
        ":indeterminate",
        ":valid",
        ":invalid",
        ":user-valid",
        ":user-invalid",
        ":in-range",
        ":out-of-range",
        ":required",
        ":optional",
        ":autofill",
        ":open",
        ":modal",
        ":defined",
        ":dir()",
        ":lang()",
        ":is()",
        ":where()",
        ":not()",
        ":has()"
      ]
    },
    "tags": [
      {
        "children": [
          "div",
          "span",
          "p",
          "h1",
          "h2",
          "h3",
          "ul",
          "ol",
          "li",
          "header",
          "nav",
          "main",
          "section",
          "footer",
          "strong",
          "em",
          "code",
          "pre",
          "br",
          "button",
          "a",
          "form",
          "label",
          "input",
          "textarea",
          "select",
          "option",
          "dialog",
          "progress",
          "img",
          "aside"
        ],
        "code": 1,
        "name": "div",
        "textChild": true
      },
      {
        "children": [
          "div",
          "span",
          "p",
          "h1",
          "h2",
          "h3",
          "ul",
          "ol",
          "li",
          "header",
          "nav",
          "main",
          "section",
          "footer",
          "strong",
          "em",
          "code",
          "pre",
          "br",
          "button",
          "a",
          "form",
          "label",
          "input",
          "textarea",
          "select",
          "option",
          "dialog",
          "progress",
          "img",
          "aside"
        ],
        "code": 2,
        "name": "span",
        "textChild": true
      },
      {
        "children": [
          "div",
          "span",
          "p",
          "h1",
          "h2",
          "h3",
          "ul",
          "ol",
          "li",
          "header",
          "nav",
          "main",
          "section",
          "footer",
          "strong",
          "em",
          "code",
          "pre",
          "br",
          "button",
          "a",
          "form",
          "label",
          "input",
          "textarea",
          "select",
          "option",
          "dialog",
          "progress",
          "img",
          "aside"
        ],
        "code": 3,
        "name": "p",
        "textChild": true
      },
      {
        "children": [
          "div",
          "span",
          "p",
          "h1",
          "h2",
          "h3",
          "ul",
          "ol",
          "li",
          "header",
          "nav",
          "main",
          "section",
          "footer",
          "strong",
          "em",
          "code",
          "pre",
          "br",
          "button",
          "a",
          "form",
          "label",
          "input",
          "textarea",
          "select",
          "option",
          "dialog",
          "progress",
          "img",
          "aside"
        ],
        "code": 4,
        "name": "h1",
        "textChild": true
      },
      {
        "children": [
          "div",
          "span",
          "p",
          "h1",
          "h2",
          "h3",
          "ul",
          "ol",
          "li",
          "header",
          "nav",
          "main",
          "section",
          "footer",
          "strong",
          "em",
          "code",
          "pre",
          "br",
          "button",
          "a",
          "form",
          "label",
          "input",
          "textarea",
          "select",
          "option",
          "dialog",
          "progress",
          "img",
          "aside"
        ],
        "code": 5,
        "name": "h2",
        "textChild": true
      },
      {
        "children": [
          "div",
          "span",
          "p",
          "h1",
          "h2",
          "h3",
          "ul",
          "ol",
          "li",
          "header",
          "nav",
          "main",
          "section",
          "footer",
          "strong",
          "em",
          "code",
          "pre",
          "br",
          "button",
          "a",
          "form",
          "label",
          "input",
          "textarea",
          "select",
          "option",
          "dialog",
          "progress",
          "img",
          "aside"
        ],
        "code": 6,
        "name": "h3",
        "textChild": true
      },
      {
        "children": [
          "div",
          "span",
          "p",
          "h1",
          "h2",
          "h3",
          "ul",
          "ol",
          "li",
          "header",
          "nav",
          "main",
          "section",
          "footer",
          "strong",
          "em",
          "code",
          "pre",
          "br",
          "button",
          "a",
          "form",
          "label",
          "input",
          "textarea",
          "select",
          "option",
          "dialog",
          "progress",
          "img",
          "aside"
        ],
        "code": 7,
        "name": "ul",
        "textChild": true
      },
      {
        "children": [
          "div",
          "span",
          "p",
          "h1",
          "h2",
          "h3",
          "ul",
          "ol",
          "li",
          "header",
          "nav",
          "main",
          "section",
          "footer",
          "strong",
          "em",
          "code",
          "pre",
          "br",
          "button",
          "a",
          "form",
          "label",
          "input",
          "textarea",
          "select",
          "option",
          "dialog",
          "progress",
          "img",
          "aside"
        ],
        "code": 8,
        "name": "ol",
        "textChild": true
      },
      {
        "children": [
          "div",
          "span",
          "p",
          "h1",
          "h2",
          "h3",
          "ul",
          "ol",
          "li",
          "header",
          "nav",
          "main",
          "section",
          "footer",
          "strong",
          "em",
          "code",
          "pre",
          "br",
          "button",
          "a",
          "form",
          "label",
          "input",
          "textarea",
          "select",
          "option",
          "dialog",
          "progress",
          "img",
          "aside"
        ],
        "code": 9,
        "name": "li",
        "textChild": true
      },
      {
        "children": [
          "div",
          "span",
          "p",
          "h1",
          "h2",
          "h3",
          "ul",
          "ol",
          "li",
          "header",
          "nav",
          "main",
          "section",
          "footer",
          "strong",
          "em",
          "code",
          "pre",
          "br",
          "button",
          "a",
          "form",
          "label",
          "input",
          "textarea",
          "select",
          "option",
          "dialog",
          "progress",
          "img",
          "aside"
        ],
        "code": 10,
        "name": "header",
        "textChild": true
      },
      {
        "children": [
          "div",
          "span",
          "p",
          "h1",
          "h2",
          "h3",
          "ul",
          "ol",
          "li",
          "header",
          "nav",
          "main",
          "section",
          "footer",
          "strong",
          "em",
          "code",
          "pre",
          "br",
          "button",
          "a",
          "form",
          "label",
          "input",
          "textarea",
          "select",
          "option",
          "dialog",
          "progress",
          "img",
          "aside"
        ],
        "code": 11,
        "name": "nav",
        "textChild": true
      },
      {
        "children": [
          "div",
          "span",
          "p",
          "h1",
          "h2",
          "h3",
          "ul",
          "ol",
          "li",
          "header",
          "nav",
          "main",
          "section",
          "footer",
          "strong",
          "em",
          "code",
          "pre",
          "br",
          "button",
          "a",
          "form",
          "label",
          "input",
          "textarea",
          "select",
          "option",
          "dialog",
          "progress",
          "img",
          "aside"
        ],
        "code": 12,
        "name": "main",
        "textChild": true
      },
      {
        "children": [
          "div",
          "span",
          "p",
          "h1",
          "h2",
          "h3",
          "ul",
          "ol",
          "li",
          "header",
          "nav",
          "main",
          "section",
          "footer",
          "strong",
          "em",
          "code",
          "pre",
          "br",
          "button",
          "a",
          "form",
          "label",
          "input",
          "textarea",
          "select",
          "option",
          "dialog",
          "progress",
          "img",
          "aside"
        ],
        "code": 13,
        "name": "section",
        "textChild": true
      },
      {
        "children": [
          "div",
          "span",
          "p",
          "h1",
          "h2",
          "h3",
          "ul",
          "ol",
          "li",
          "header",
          "nav",
          "main",
          "section",
          "footer",
          "strong",
          "em",
          "code",
          "pre",
          "br",
          "button",
          "a",
          "form",
          "label",
          "input",
          "textarea",
          "select",
          "option",
          "dialog",
          "progress",
          "img",
          "aside"
        ],
        "code": 14,
        "name": "footer",
        "textChild": true
      },
      {
        "children": [
          "div",
          "span",
          "p",
          "h1",
          "h2",
          "h3",
          "ul",
          "ol",
          "li",
          "header",
          "nav",
          "main",
          "section",
          "footer",
          "strong",
          "em",
          "code",
          "pre",
          "br",
          "button",
          "a",
          "form",
          "label",
          "input",
          "textarea",
          "select",
          "option",
          "dialog",
          "progress",
          "img",
          "aside"
        ],
        "code": 15,
        "name": "strong",
        "textChild": true
      },
      {
        "children": [
          "div",
          "span",
          "p",
          "h1",
          "h2",
          "h3",
          "ul",
          "ol",
          "li",
          "header",
          "nav",
          "main",
          "section",
          "footer",
          "strong",
          "em",
          "code",
          "pre",
          "br",
          "button",
          "a",
          "form",
          "label",
          "input",
          "textarea",
          "select",
          "option",
          "dialog",
          "progress",
          "img",
          "aside"
        ],
        "code": 16,
        "name": "em",
        "textChild": true
      },
      {
        "children": [
          "div",
          "span",
          "p",
          "h1",
          "h2",
          "h3",
          "ul",
          "ol",
          "li",
          "header",
          "nav",
          "main",
          "section",
          "footer",
          "strong",
          "em",
          "code",
          "pre",
          "br",
          "button",
          "a",
          "form",
          "label",
          "input",
          "textarea",
          "select",
          "option",
          "dialog",
          "progress",
          "img",
          "aside"
        ],
        "code": 17,
        "name": "code",
        "textChild": true
      },
      {
        "children": [
          "div",
          "span",
          "p",
          "h1",
          "h2",
          "h3",
          "ul",
          "ol",
          "li",
          "header",
          "nav",
          "main",
          "section",
          "footer",
          "strong",
          "em",
          "code",
          "pre",
          "br",
          "button",
          "a",
          "form",
          "label",
          "input",
          "textarea",
          "select",
          "option",
          "dialog",
          "progress",
          "img",
          "aside"
        ],
        "code": 18,
        "name": "pre",
        "textChild": true
      },
      {
        "children": [],
        "code": 19,
        "name": "br",
        "textChild": false
      },
      {
        "children": [
          "div",
          "span",
          "p",
          "h1",
          "h2",
          "h3",
          "ul",
          "ol",
          "li",
          "header",
          "nav",
          "main",
          "section",
          "footer",
          "strong",
          "em",
          "code",
          "pre",
          "br",
          "button",
          "a",
          "form",
          "label",
          "input",
          "textarea",
          "select",
          "option",
          "dialog",
          "progress",
          "img",
          "aside"
        ],
        "code": 20,
        "name": "button",
        "textChild": true
      },
      {
        "children": [
          "div",
          "span",
          "p",
          "h1",
          "h2",
          "h3",
          "ul",
          "ol",
          "li",
          "header",
          "nav",
          "main",
          "section",
          "footer",
          "strong",
          "em",
          "code",
          "pre",
          "br",
          "button",
          "a",
          "form",
          "label",
          "input",
          "textarea",
          "select",
          "option",
          "dialog",
          "progress",
          "img",
          "aside"
        ],
        "code": 21,
        "name": "a",
        "textChild": true
      },
      {
        "children": [
          "div",
          "span",
          "p",
          "h1",
          "h2",
          "h3",
          "ul",
          "ol",
          "li",
          "header",
          "nav",
          "main",
          "section",
          "footer",
          "strong",
          "em",
          "code",
          "pre",
          "br",
          "button",
          "a",
          "form",
          "label",
          "input",
          "textarea",
          "select",
          "option",
          "dialog",
          "progress",
          "img",
          "aside"
        ],
        "code": 22,
        "name": "form",
        "textChild": true
      },
      {
        "children": [
          "div",
          "span",
          "p",
          "h1",
          "h2",
          "h3",
          "ul",
          "ol",
          "li",
          "header",
          "nav",
          "main",
          "section",
          "footer",
          "strong",
          "em",
          "code",
          "pre",
          "br",
          "button",
          "a",
          "form",
          "label",
          "input",
          "textarea",
          "select",
          "option",
          "dialog",
          "progress",
          "img",
          "aside"
        ],
        "code": 23,
        "name": "label",
        "textChild": true
      },
      {
        "children": [],
        "code": 24,
        "name": "input",
        "textChild": false
      },
      {
        "children": [],
        "code": 25,
        "name": "textarea",
        "textChild": false
      },
      {
        "children": [
          "option"
        ],
        "code": 26,
        "name": "select",
        "textChild": false
      },
      {
        "children": [],
        "code": 27,
        "name": "option",
        "textChild": true
      },
      {
        "children": [
          "div",
          "span",
          "p",
          "h1",
          "h2",
          "h3",
          "ul",
          "ol",
          "li",
          "header",
          "nav",
          "main",
          "section",
          "footer",
          "strong",
          "em",
          "code",
          "pre",
          "br",
          "button",
          "a",
          "form",
          "label",
          "input",
          "textarea",
          "select",
          "option",
          "dialog",
          "progress",
          "img",
          "aside"
        ],
        "code": 28,
        "name": "dialog",
        "textChild": true
      },
      {
        "children": [],
        "code": 29,
        "name": "progress",
        "textChild": false
      },
      {
        "children": [],
        "code": 30,
        "name": "img",
        "textChild": false
      },
      {
        "children": [
          "div",
          "span",
          "p",
          "h1",
          "h2",
          "h3",
          "ul",
          "ol",
          "li",
          "header",
          "nav",
          "main",
          "section",
          "footer",
          "strong",
          "em",
          "code",
          "pre",
          "br",
          "button",
          "a",
          "form",
          "label",
          "input",
          "textarea",
          "select",
          "option",
          "dialog",
          "progress",
          "img",
          "aside"
        ],
        "code": 31,
        "name": "aside",
        "textChild": true
      }
    ],
    "wire": {
      "alignment": 4,
      "batchHeaderBytes": 8,
      "commitImport": "view_commit",
      "eventExport": "varta.view_abi.event",
      "eventFields": [
        {
          "name": "version",
          "offset": 0,
          "type": "u16"
        },
        {
          "name": "kind",
          "offset": 2,
          "type": "u16"
        },
        {
          "name": "handler",
          "offset": 4,
          "type": "u32"
        },
        {
          "name": "target",
          "offset": 8,
          "type": "u32"
        },
        {
          "name": "seq",
          "offset": 12,
          "type": "u32"
        },
        {
          "name": "flags",
          "offset": 16,
          "type": "u32"
        },
        {
          "name": "status",
          "offset": 20,
          "type": "u32"
        },
        {
          "name": "scroll_top",
          "offset": 24,
          "type": "f64"
        },
        {
          "name": "scroll_height",
          "offset": 32,
          "type": "f64"
        },
        {
          "name": "client_height",
          "offset": 40,
          "type": "f64"
        }
      ],
      "eventHeaderBytes": 48,
      "eventStrings": [
        "key",
        "value"
      ],
      "noNode": 0,
      "operationHeaderBytes": 4,
      "root": 1,
      "version": 0
    }
  }
});
