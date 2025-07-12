#include <foxglove/arena.hpp>

#include <catch2/catch_test_macros.hpp>
#include <catch2/matchers/catch_matchers_string.hpp>

#include <string>

using Catch::Matchers::ContainsSubstring;
using Catch::Matchers::Equals;

TEST_CASE("allocate different types from arena and verify alignment") {
  foxglove::Arena arena;

  // Allocate different types and verify alignment
  int* intPtr = arena.alloc<int>(10);
  REQUIRE(reinterpret_cast<uintptr_t>(intPtr) % alignof(int) == 0);

  double* doublePtr = arena.alloc<double>(5);
  REQUIRE(reinterpret_cast<uintptr_t>(doublePtr) % alignof(double) == 0);

  struct AlignedStruct {
    alignas(16) char data[32];
  };

  AlignedStruct* structPtr = arena.alloc<AlignedStruct>(3);
  REQUIRE(reinterpret_cast<uintptr_t>(structPtr) % alignof(AlignedStruct) == 0);

  // Verify we can write to the allocated memory
  for (int i = 0; i < 10; i++) {
    intPtr[i] = i;
  }

  for (int i = 0; i < 5; i++) {
    doublePtr[i] = i * 1.5;
  }

  // Verify the values were written correctly
  for (int i = 0; i < 10; i++) {
    REQUIRE(intPtr[i] == i);
  }

  for (int i = 0; i < 5; i++) {
    REQUIRE(doublePtr[i] == i * 1.5);
  }
}

TEST_CASE("allocate from heap when arena capacity is exceeded") {
  foxglove::Arena arena;

  // First, nearly fill the arena
  constexpr size_t nearlyFullSize = foxglove::Arena::Size - 1024;
  char* buffer = arena.alloc<char>(nearlyFullSize);
  REQUIRE(buffer != nullptr);

  // Verify some data can be written to the arena allocation
  buffer[0] = 'A';
  buffer[nearlyFullSize - 1] = 'Z';
  REQUIRE(buffer[0] == 'A');
  REQUIRE(buffer[nearlyFullSize - 1] == 'Z');

  // Check arena's reported space
  REQUIRE(arena.used() >= nearlyFullSize);
  REQUIRE(arena.available() == 1024);

  // Now allocate more than what's left in the arena
  constexpr size_t largeAllocationSize = 8192;
  int* largeAllocation = arena.alloc<int>(largeAllocationSize / sizeof(int));
  REQUIRE(largeAllocation != nullptr);

  // Verify we can use the overflow allocation
  for (size_t i = 0; i < largeAllocationSize / sizeof(int); i++) {
    largeAllocation[i] = static_cast<int>(i);
  }

  // Make several more overflow allocations
  uint32_t* overflow1 = arena.alloc<uint32_t>(1000);
  uint64_t* overflow2 = arena.alloc<uint64_t>(2000);

  REQUIRE(overflow1 != nullptr);
  REQUIRE(overflow2 != nullptr);

  // Verify each allocation can be written to
  overflow1[0] = 1234567890;
  overflow2[0] = 1234567890123456789;

  REQUIRE(overflow1[0] == 1234567890);
  REQUIRE(overflow2[0] == 1234567890123456789);
}
