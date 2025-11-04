# Higher-Order Functions and Collection Utilities

This document describes the higher-order functions and collection utilities available in slisp (currently interpreter-only).

## Higher-Order Functions

All higher-order functions work polymorphically across vectors, sets, and maps.

### map
Apply a function to each element of a collection. Returns a vector.

```lisp
(defn double [x] (* x 2))
(map double [1 2 3 4])  ; => [2 4 6 8]
(map double #{1 2 3})   ; => [2 4 6] (order may vary)

; For maps, the function receives [key value] pairs
(defn get-value [pair] (get pair 1))
(map get-value {:a 1 :b 2})  ; => [1 2]
```

### filter
Select elements that satisfy a predicate. Preserves collection type.

```lisp
(defn is-positive [x] (> x 0))
(filter is-positive [1 -2 3 -4 5])  ; => [1 3 5]
(filter is-positive #{1 -2 3 -4})   ; => #{1 3}

; For maps, the predicate receives [key value] pairs and returns a map
(defn value-gt-1 [pair] (> (get pair 1) 1))
(filter value-gt-1 {:a 1 :b 2 :c 3})  ; => {:b 2 :c 3}
```

### reduce
Fold/accumulate over a collection. Works with vectors, sets, and maps.

```lisp
(defn add [a b] (+ a b))
(reduce add 0 [1 2 3 4 5])  ; => 15
(reduce add 0 #{1 2 3})     ; => 6

; Without initial value (uses first element)
(reduce add [1 2 3 4 5])  ; => 15

; For maps, the function receives [key value] pairs
(defn sum-values [acc pair] (+ acc (get pair 1)))
(reduce sum-values 0 {:a 1 :b 2 :c 3})  ; => 6
```

## Collection Operations

### first
Get the first element of a collection (returns `nil` if empty). Works with vectors, sets, and maps.

```lisp
(first [1 2 3])     ; => 1
(first #{1 2 3})    ; => 1 (arbitrary element)
(first {:a 1 :b 2}) ; => [:a 1] (arbitrary entry)
(first [])          ; => nil
```

### rest
Get all but the first element of a collection. Preserves collection type.

```lisp
(rest [1 2 3 4])    ; => [2 3 4]
(rest #{1 2 3})     ; => #{2 3} (minus arbitrary first)
(rest {:a 1 :b 2})  ; => {:b 2} (minus arbitrary first)
(rest [])           ; => []
```

### cons
Add element to the front of a collection (vectors only).

```lisp
(cons 1 [2 3 4])  ; => [1 2 3 4]
```

### conj
Conjoin element(s) to a collection (append for vectors).

```lisp
(conj [1 2] 3 4 5)  ; => [1 2 3 4 5]
```

### concat
Concatenate multiple collections. Always returns a vector. Works with any collection type.

```lisp
(concat [1 2] [3 4] [5 6])  ; => [1 2 3 4 5 6]
(concat [1 2] #{3 4})       ; => [1 2 3 4]
(concat {:a 1} {:b 2})      ; => [[:a 1] [:b 2]]
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
