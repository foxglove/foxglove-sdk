#include <foxglove/arena.hpp>

#include <catch2/catch_test_macros.hpp>
#include <catch2/matchers/catch_matchers_string.hpp>

#include <array>
#include <string>

using Catch::Matchers::ContainsSubstring;
using Catch::Matchers::Equals;

TEST_CASE("allocate different types from arena and verify alignment") {
  foxglove::Arena arena;

  // Allocate different types and verify alignment
  auto* int_ptr = arena.alloc<int>(10);
  REQUIRE(reinterpret_cast<uintptr_t>(int_ptr) % alignof(int) == 0);

  auto* double_ptr = arena.alloc<double>(5);
  REQUIRE(reinterpret_cast<uintptr_t>(double_ptr) % alignof(double) == 0);

  struct AlignedStruct {
    alignas(16) std::array<char, 32> data;
  };

  auto* struct_ptr = arena.alloc<AlignedStruct>(3);
  REQUIRE(reinterpret_cast<uintptr_t>(struct_ptr) % alignof(AlignedStruct) == 0);

  // Verify we can write to the allocated memory
  for (int i = 0; i < 10; i++) {
    int_ptr[i] = i;
  }

  for (int i = 0; i < 5; i++) {
    double_ptr[i] = i * 1.5;
  }

  // Verify the values were written correctly
  for (int i = 0; i < 10; i++) {
    REQUIRE(int_ptr[i] == i);
  }

  for (int i = 0; i < 5; i++) {
    REQUIRE(double_ptr[i] == i * 1.5);
  }
}

TEST_CASE("allocate from heap when arena capacity is exceeded") {
  foxglove::Arena arena;

  // First, nearly fill the arena
  constexpr size_t nearly_full_size = foxglove::Arena::size - 1024;
  char* buffer = arena.alloc<char>(nearly_full_size);
  REQUIRE(buffer != nullptr);

  // Verify some data can be written to the arena allocation
  buffer[0] = 'A';
  buffer[nearly_full_size - 1] = 'Z';
  REQUIRE(buffer[0] == 'A');
  REQUIRE(buffer[nearly_full_size - 1] == 'Z');

  // Check arena's reported space
  REQUIRE(arena.used() >= nearly_full_size);
  REQUIRE(arena.available() == 1024);

  // Now allocate more than what's left in the arena
  constexpr size_t large_allocation_size = 8192;
  auto* large_allocation = arena.alloc<int>(large_allocation_size / sizeof(int));
  REQUIRE(large_allocation != nullptr);

  // Verify we can use the overflow allocation
  for (size_t i = 0; i < large_allocation_size / sizeof(int); i++) {
    large_allocation[i] = static_cast<int>(i);
  }

  // Make several more overflow allocations
  auto* overflow1 = arena.alloc<uint32_t>(1000);
  auto* overflow2 = arena.alloc<uint64_t>(2000);

  REQUIRE(overflow1 != nullptr);
  REQUIRE(overflow2 != nullptr);

  // Verify each allocation can be written to
  overflow1[0] = 1234567890;
  overflow2[0] = 1234567890123456789;

  REQUIRE(overflow1[0] == 1234567890);
  REQUIRE(overflow2[0] == 1234567890123456789);
}
