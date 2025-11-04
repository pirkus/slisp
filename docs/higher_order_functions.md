# Higher-Order Functions and Collection Utilities

This document describes the higher-order functions and collection utilities available in slisp (currently interpreter-only).

## Higher-Order Functions

### map
Apply a function to each element of a collection.

```lisp
(defn double [x] (* x 2))
(map double [1 2 3 4])  ; => [2 4 6 8]
```

### filter
Select elements that satisfy a predicate.

```lisp
(defn is-positive [x] (> x 0))
(filter is-positive [1 -2 3 -4 5])  ; => [1 3 5]
```

### reduce
Fold/accumulate over a collection.

```lisp
(defn add [a b] (+ a b))
(reduce add 0 [1 2 3 4 5])  ; => 15

; Without initial value (uses first element)
(reduce add [1 2 3 4 5])  ; => 15
```

## Collection Operations

### first
Get the first element of a collection (returns `nil` if empty).

```lisp
(first [1 2 3])  ; => 1
(first [])       ; => nil
```

### rest
Get all but the first element of a collection.

```lisp
(rest [1 2 3 4])  ; => [2 3 4]
(rest [])         ; => []
```

### cons
Add element to the front of a collection.

```lisp
(cons 1 [2 3 4])  ; => [1 2 3 4]
```

### conj
Conjoin element(s) to a collection (append for vectors).

```lisp
(conj [1 2] 3 4 5)  ; => [1 2 3 4 5]
```

### concat
Concatenate multiple collections.

```lisp
(concat [1 2] [3 4] [5 6])  ; => [1 2 3 4 5 6]
```

## Map Utilities

### keys
Get all keys from a map as a vector.

```lisp
(keys {:a 1 :b 2 :c 3})  ; => [:a :b :c] (order may vary)
```

### vals
Get all values from a map as a vector.

```lisp
(vals {:a 1 :b 2 :c 3})  ; => [1 2 3] (order may vary)
```

### merge
Merge multiple maps (right-associative - later values override earlier ones).

```lisp
(merge {:a 1 :b 2} {:b 3 :c 4})  ; => {:a 1 :b 3 :c 4}
```

### select-keys
Select a subset of keys from a map.

```lisp
(select-keys {:a 1 :b 2 :c 3} [:a :c])  ; => {:a 1 :c 3}
```

### zipmap
Create a map from a vector of keys and a vector of values.

```lisp
(zipmap [:a :b :c] [1 2 3])  ; => {:a 1 :b 2 :c 3}
```

## Examples

### Pipeline Transformation

```lisp
(defn double [x] (* x 2))
(defn is-even [x] (= 0 (- x (* 2 (/ x 2)))))
(defn sum [a b] (+ a b))

(defn -main []
  (let [numbers [1 2 3 4 5 6 7 8 9 10]
        doubled (map double numbers)
        evens (filter is-even doubled)
        total (reduce sum 0 evens)]
    total))  ; => 60
```

### Map Manipulation

```lisp
(defn -main []
  (let [person {:name "Alice" :age 30 :city "NYC"}
        subset (select-keys person [:name :city])
        updated (merge subset {:age 31 :country "USA"})]
    (count (keys updated))))  ; => 4
```

### Building Collections

```lisp
(defn -main []
  (let [names ["Alice" "Bob" "Charlie"]
        ids [1 2 3]
        name-map (zipmap ids names)
        all-keys (keys name-map)
        all-vals (vals name-map)]
    (concat all-keys all-vals)))  ; => [1 2 3 "Alice" "Bob" "Charlie"]
```

## Notes

- These functions are currently available in **interpreter mode only**
- Compiler support requires function value passing (planned for future phases)
- All functions handle `nil` gracefully (typically treating it as an empty collection)
- Higher-order functions clone values when necessary to maintain ownership semantics
- Map iteration order is not guaranteed (backed by HashMap)
