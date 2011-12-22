// Top-level, visible-everywhere definitions.

// Export type option as a synonym for option::t and export the some and none
// tag constructors.

import option::{some,  none};
import option = option::t;
export option, some, none;

// Export the log levels as global constants. Higher levels mean
// more-verbosity. Error is the bottom level, default logging level is
// warn-and-below.

export error, warn, info, debug;
const error : u32 = 0_u32;
const warn : u32 = 1_u32;
const info : u32 = 2_u32;
const debug : u32 = 3_u32;
