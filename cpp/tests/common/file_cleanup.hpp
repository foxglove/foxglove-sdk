#pragma once

#include <filesystem>
#include <string>

namespace foxglove_tests {

class FileCleanup {
public:
  explicit FileCleanup(std::string&& path)
      : path_(std::move(path)) {}
  FileCleanup(const FileCleanup&) = delete;
  FileCleanup& operator=(const FileCleanup&) = delete;
  FileCleanup(FileCleanup&&) = delete;
  FileCleanup& operator=(FileCleanup&&) = delete;
  ~FileCleanup() {
    if (std::filesystem::exists(path_)) {
      std::filesystem::remove(path_);
    }
  }

private:
  std::string path_;
};

}  // namespace foxglove_tests
