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
/// If the arena runs out of space, it attempts to allocate the required space on the heap.
/// If this fails, it throws std::bad_alloc(). On wasm32 platforms which do not support exceptions,
/// It calls std::terminate().
/// The allocated arrays are "freed" by dropping the arena, destructors are not run.
/// @cond foxglove_internal
class Arena {
public:
  static constexpr std::size_t kSize = static_cast<std::size_t>(8) * 1024;  // 8 KB

  Arena() = default;

  /// Maps elements from a vector to a new array allocated from the arena.
  ///
  /// @param src The source vector containing elements to map
  /// @param map_fn Function taking (T& dest, const S& src) to map elements.
  /// T must be a POD type, without a custom constructor or destructor.
  /// @return Pointer to the beginning of the allocated array of src.size() T elements, or null if
  /// elements is 0.
  /// @throws std::bad_alloc if the the fallback to heap allocation fails.
  /// On wasm32 platforms which do not support exceptions, calls std::terminate().
  template<
    typename T, typename S, typename Fn,
    typename = std::enable_if_t<std::is_pod_v<T> && std::is_invocable_v<Fn, T&, const S&, Arena&>>>
  // NOLINTNEXTLINE(cppcoreguidelines-missing-std-forward)
  T* map(const std::vector<S>& src, Fn&& map_fn) {
    const size_t elements = src.size();
    if (elements == 0) {
      return nullptr;
    }
    T* result = alloc<T>(elements);
    T* current = result;

    // Convert the elements from S to T, placing them in the result array
    for (auto it = src.begin(); it != src.end(); ++it) {
      map_fn(*current++, *it, *this);
    }

    return result;
  }

  /// Map a single source object of type S to a new object of type T allocated from the arena.
  ///
  /// @param src The source object to map
  /// @param map_fn Function taking (T& dest, const S& src) to map the element.
  /// T must be a POD type, without a custom constructor or destructor.
  /// @return Pointer to the newly allocated T object
  /// @throws std::bad_alloc if the the fallback to heap allocation fails.
  /// On wasm32 platforms which do not support exceptions, calls std::terminate().
  template<
    typename T, typename S, typename Fn,
    typename = std::enable_if_t<std::is_pod_v<T> && std::is_invocable_v<Fn, T&, const S&, Arena&>>>
  T* mapOne(const S& src, Fn&& map_fn) {
    T* result = alloc<T>(1);
    std::forward<Fn>(map_fn)(*result, src, *this);
    return result;
  }

  /// Allocates memory for an object of type T from the arena.
  ///
  /// @param elements Number of elements to allocate
  /// @return Pointer to the aligned memory for the requested elements
  /// @throws std::bad_alloc if the the fallback to heap allocation fails.
  /// On wasm32 platforms which do not support exceptions, calls std::terminate().
  template<typename T>
  T* alloc(size_t elements) {
    assert(elements > 0);
    const size_t bytes_needed = elements * sizeof(T);
    const size_t alignment = alignof(T);

    // Calculate space available in the buffer
    size_t space_left = available();
    // NOLINTNEXTLINE(cppcoreguidelines-pro-bounds-constant-array-index)
    void* buffer_ptr = &buffer_[offset_];

    // Align the pointer within the buffer
    void* aligned_ptr = std::align(alignment, bytes_needed, buffer_ptr, space_left);

    // Check if we have enough space
    if (aligned_ptr == nullptr) {
      // We don't use aligned_alloc because it fails on some platforms for larger alignments
      size_t size_with_alignment = alignment + bytes_needed;
      // NOLINTBEGIN(cppcoreguidelines-no-malloc,hicpp-no-malloc,cppcoreguidelines-owning-memory)
      auto* ptr = ::malloc(size_with_alignment);
      // NOLINTNEXTLINE(clang-analyzer-unix.Malloc)
      aligned_ptr = std::align(alignment, bytes_needed, ptr, size_with_alignment);
      if (aligned_ptr == nullptr) {
#ifndef __wasm32__
        throw std::bad_alloc();
#else
        std::terminate();
#endif
      }
      overflow_.emplace_back(static_cast<char*>(ptr));
      // NOLINTEND(cppcoreguidelines-no-malloc,hicpp-no-malloc,cppcoreguidelines-owning-memory)
      return reinterpret_cast<T*>(aligned_ptr);
    }

    // Calculate the new offset
    offset_ = kSize - space_left + bytes_needed;
    return reinterpret_cast<T*>(aligned_ptr);
  }

  /// Returns how many bytes are currently used in the arena.
  [[nodiscard]] size_t used() const {
    return offset_;
  }

  /// Returns how many bytes are available in the arena.
  [[nodiscard]] size_t available() const {
    return kSize - offset_;
  }

private:
  struct Deleter {
    void operator()(char* ptr) const {
      // NOLINTNEXTLINE(cppcoreguidelines-no-malloc,hicpp-no-malloc,cppcoreguidelines-owning-memory)
      free(ptr);
    }
  };

  std::array<uint8_t, kSize> buffer_{};
  std::size_t offset_ = 0;
  std::vector<std::unique_ptr<char, Deleter>> overflow_;
};
/// @endcond

}  // namespace foxglove
