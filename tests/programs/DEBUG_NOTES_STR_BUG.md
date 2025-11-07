# String Concatenation Bug in Function Parameters

## Summary
When `str` concatenates strings that are passed as function parameters, it includes extra metadata bytes in the result, causing incorrect string lengths.

## Test Results

### Pattern Discovery
```
concat1(a): str(a) where a="hello" (5 chars)
  Expected: 5
  Actual: 7
  Extra: +2 bytes

concat2(a,b): str(a, b) where a="he", b="llo" (2+3=5 chars total)
  Expected: 5
  Actual: 14  
  Extra: +9 bytes

concat3(a,b,c): str(a, b, c) where a="h", b="el", c="lo" (1+2+2=5 chars total)
  Expected: 5
  Actual: 21
  Extra: +16 bytes
```

### What Works Correctly
- String literals returned directly from functions
- `str` with literal arguments (not parameters)
- String operations done inline in -main

### What Fails
- `str` concatenating function parameters
- Functions returning strings created from parameters

## Root Cause Hypothesis
The `str` function is including string metadata (likely 2-byte length prefixes) from parameter strings instead of just their data. Each parameter string contributes its metadata to the final concatenated result.

## Impact
- 3 tests failing: churn_reuse, map_churn, mixed_sizes
- All tests that use `str` with parameters will compute incorrect values

## Location to Investigate
- Runtime string concatenation implementation (`str` function)
- How parameter strings are represented/passed in the calling convention
- String metadata handling in concatenation operations

## Reproduction
```slisp
(defn concat2 [a b]
  (str a b))

(defn -main []
  (count (concat2 "he" "llo")))  ; Returns 14 instead of 5
```
