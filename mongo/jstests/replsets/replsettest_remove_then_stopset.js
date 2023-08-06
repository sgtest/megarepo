/**
 * Tests that it is safe to call stopSet() after a remove() in ReplSetTest.
 */

const replTest = new ReplSetTest({nodes: 1});
replTest.startSet();
replTest.remove(0);
replTest.stopSet();