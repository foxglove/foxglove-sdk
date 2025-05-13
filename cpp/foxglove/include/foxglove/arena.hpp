#pragma once

#include <cstddef>
#include <cstdint>
#include <cstdlib>
#include <memory>
#include <new>
#include <type_traits>
#include <vector>

namespace foxglove {

/**
 * A fixed-size memory arena that allocates aligned arrays of POD types on the stack.
 * The arena contains a single inline array and allocates from it.
 * If the arena runs out of space, it throws std::bad_alloc.
 * The allocated arrays are "freed" by dropping the arena, destructors are not run.
 */
class Arena {
public:
  static constexpr std::size_t Size = 128 * 1024;  // 128 KB

  Arena()
      : _offset(0) {}

  /**
   * Maps elements from a vector to a new array allocated from the arena.
   *
   * @param src The source vector containing elements to map
   * @param map_fn Function taking (T& dest, const S& src) to map elements.
   * T must be a POD type, without a custom constructor or destructor.
   * @return Pointer to the beginning of the allocated array of src.size() T elements
   * @throws std::bad_alloc if the arena doesn't have enough space
   */
  template<
    typename T, typename S, typename Fn,
    typename = std::enable_if_t<std::is_pod_v<T> && std::is_invocable_v<Fn, T&, const S&, Arena&>>>
  T* map(const std::vector<S>& src, Fn&& map_fn) {
    const size_t elements = src.size();
    T* result = alloc<T>(elements);
    T* current = result;

    // Convert the elements from S to T, placing them in the result array
    for (auto it = src.begin(); it != src.end(); ++it) {
      map_fn(*current++, *it, *this);
    }

    return result;
  }

  /**
   * Map a single source object of type S to a new object of type T allocated from the arena.
   *
   * @param src The source vector containing elements to map
   * @param map_fn Function taking (T& dest, const S& src) to map elements.
   * T must be a POD type, without a custom constructor or destructor.
   * @return Pointer to the beginning of the allocated array of src.size() T elements
   * @throws std::bad_alloc if the arena doesn't have enough space
   */
  template<
    typename T, typename S, typename Fn,
    typename = std::enable_if_t<std::is_pod_v<T> && std::is_invocable_v<Fn, T&, const S&, Arena&>>>
  T* map_one(const S& src, Fn&& map_fn) {
    T* result = alloc<T>(1);
    map_fn(*result, src, *this);
    return result;
  }

  /**
   * Allocates memory for an object of type T from the arena.
   *
   * @param elements Number of elements to allocate
   * @return Pointer to the aligned memory for the requested elements
   * @throws std::bad_alloc if the arena doesn't have enough space
   */
  template<typename T>
  T* alloc(size_t elements) {
    const size_t bytes_needed = elements * sizeof(T);

    // Calculate aligned offset
    const size_t alignment = alignof(T);
    const size_t misalignment = _offset % alignment;
    const size_t alignment_padding = misalignment > 0 ? alignment - misalignment : 0;
    const size_t aligned_offset = _offset + alignment_padding;

    // Check if we have enough space
    if (aligned_offset + bytes_needed > Size) {
      _overflow.emplace_back(static_cast<char*>(::aligned_alloc(alignment, bytes_needed)));
      return reinterpret_cast<T*>(_overflow.back().get());
    }

    // Get pointer to the aligned result array of T
    T* result = reinterpret_cast<T*>(&_buffer[aligned_offset]);
    _offset = aligned_offset + bytes_needed;

    return result;
  }

  /**
   * Returns how many bytes are currently used in the arena.
   */
  size_t used() const {
    return _offset;
  }

  /**
   * Returns how many bytes are available in the arena.
   */
  size_t available() const {
    return Size - _offset;
  }

private:
  struct Deleter {
    void operator()(char* ptr) const {
      free(ptr);
    }
  };

  alignas(std::max_align_t) uint8_t _buffer[Size];
  std::size_t _offset;
  std::vector<std::unique_ptr<char, Deleter>> _overflow;
};

}  // namespace foxglove
