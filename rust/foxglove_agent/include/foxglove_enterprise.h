#include <cstdarg>
#include <cstdint>
#include <cstdlib>
#include <ostream>
#include <new>

extern "C" {

void foxglove_log_to_stdout(const uint8_t *msg, uintptr_t msg_len);

}  // extern "C"
