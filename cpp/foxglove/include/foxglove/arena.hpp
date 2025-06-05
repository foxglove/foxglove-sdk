#pragma once

#include <array>
#include <cassert>
#include <cstddef>
#include <cstdint>
#include <cstdlib>
#include <memory>
#include <new>
#include <type_traits>
#include <vector>

namespace foxglove {

/// A fixed-size memory arena that allocates aligned arrays of POD types on the stack.
/// The arena contains a single inline array and allocates from it.
/// If the arena runs out of space, it throws std::bad_alloc.
/// The allocated arrays are "freed" by dropping the arena, destructors are not run.
/// @cond foxglove_internal
class Arena {
public:
  static constexpr std::size_t Size = 128 * 1024;  // 128 KB

  Arena()
      : offset_(0) {}

  /// Maps elements from a vector to a new array allocated from the arena.
  ///
  /// @param src The source vector containing elements to map
  /// @param map_fn Function taking (T& dest, const S& src) to map elements.
  /// T must be a POD type, without a custom constructor or destructor.
  /// @return Pointer to the beginning of the allocated array of src.size() T elements, or null if
  /// elements is 0.
  /// @throws std::bad_alloc if the arena doesn't have enough space
  template<
    typename T, typename S, typename Fn,
    typename = std::enable_if_t<std::is_pod_v<T> && std::is_invocable_v<Fn, T&, const S&, Arena&>>>
  T* map(const std::vector<S>& src, Fn&& map_fn) {
    const size_t elements = src.size();
    T* result = (elements > 0) ? alloc<T>(elements) : nullptr;
    T* current = result;

    // Convert the elements from S to T, placing them in the result array
    for (auto it = src.begin(); it != src.end(); ++it) {
      map_fn(*current++, *it, *this);
    }

    return result;
  }

  /// Map a single source object of type S to a new object of type T allocated from the arena.
  ///
  /// @param src The source vector containing elements to map
  /// @param map_fn Function taking (T& dest, const S& src) to map elements.
  /// T must be a POD type, without a custom constructor or destructor.
  /// @return Pointer to the beginning of the allocated array of src.size() T elements
  /// @throws std::bad_alloc if the arena doesn't have enough space
  template<
    typename T, typename S, typename Fn,
    typename = std::enable_if_t<std::is_pod_v<T> && std::is_invocable_v<Fn, T&, const S&, Arena&>>>
  T* map_one(const S& src, Fn&& map_fn) {
    T* result = alloc<T>(1);
    map_fn(*result, src, *this);
    return result;
  }

  /// Allocates memory for an object of type T from the arena.
  ///
  /// @param elements Number of elements to allocate
  /// @return Pointer to the aligned memory for the requested elements
  /// @throws std::bad_alloc if the arena doesn't have enough space
  template<typename T>
  T* alloc(size_t elements) {
    assert(elements > 0);
    const size_t bytes_needed = elements * sizeof(T);
    const size_t alignment = alignof(T);

    // Calculate space available in the buffer
    size_t space_left = available();
    void* buffer_ptr = &buffer_[offset_];

    // Align the pointer within the buffer
    void* aligned_ptr = std::align(alignment, bytes_needed, buffer_ptr, space_left);

    // Check if we have enough space
    if (aligned_ptr == nullptr) {
      // We don't use aligned_alloc because it fails on some platforms for larger alignments
      size_t size_with_alignment = alignment + bytes_needed;
      auto ptr = ::malloc(size_with_alignment);
      aligned_ptr = std::align(alignment, bytes_needed, ptr, size_with_alignment);
      if (aligned_ptr == nullptr) {
        throw std::bad_alloc();
      }
      overflow_.emplace_back(static_cast<char*>(aligned_ptr));
      return reinterpret_cast<T*>(aligned_ptr);
    }

    // Calculate the new offset
    offset_ = Size - space_left + bytes_needed;
    return reinterpret_cast<T*>(aligned_ptr);
  }

  /// Returns how many bytes are currently used in the arena.
  size_t used() const {
    return offset_;
  }

  /// Returns how many bytes are available in the arena.
  size_t available() const {
    return Size - offset_;
  }

private:
  struct Deleter {
    void operator()(char* ptr) const {
      free(ptr);
    }
  };

  std::array<uint8_t, Size> buffer_;
  std::size_t offset_;
  std::vector<std::unique_ptr<char, Deleter>> overflow_;
};
/// @endcond

}  // namespace foxglove
