#pragma once

#include <algorithm>
#include <regex>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

namespace foxglove_bridge {

inline bool isWhitelisted(const std::string& name, const std::vector<std::regex>& regexPatterns) {
  return std::find_if(regexPatterns.begin(), regexPatterns.end(), [name](const auto& regex) {
           return std::regex_match(name, regex);
         }) != regexPatterns.end();
}

inline std::pair<std::string, std::string> getNodeAndNodeNamespace(const std::string& fqnNodeName) {
  const std::size_t found = fqnNodeName.find_last_of("/");
  if (found == std::string::npos) {
    throw std::runtime_error("Invalid fully qualified node name: " + fqnNodeName);
  }
  return std::make_pair(fqnNodeName.substr(0, found), fqnNodeName.substr(found + 1));
}

inline std::string trimString(std::string& str) {
  constexpr char whitespaces[] = "\t\n\r ";
  str.erase(0, str.find_first_not_of(whitespaces));  // trim left
  str.erase(str.find_last_not_of(whitespaces) + 1);  // trim right
  return str;
}

inline std::vector<std::string> splitMessageDefinitions(std::istream& stream) {
  std::vector<std::string> definitions;

  std::string line = "";
  std::string definition = "";

  while (std::getline(stream, line)) {
    line = trimString(line);
    if (line == "---") {
      definitions.push_back(trimString(definition));
      definition = "";
    } else {
      definition += line + "\n";
    }
  }

  definitions.push_back(trimString(definition));
  return definitions;
}

}  // namespace foxglove_bridge
