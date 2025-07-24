
#include "bindings/data_loader.hpp"
using namespace foxglove_data_loader;
class MyDataLoader: public foxglove_data_loader::AbstractDataLoader {

    Result<Initialization> initialize() override {
      return Result<Initialization>::error_with_message("not implemented");
    }
    Result<AbstractMessageIterator*> create_iterator(const MessageIteratorArgs& args) override {
      return Result<AbstractMessageIterator*>::error_with_message("not implemented");
    }
    Result<std::vector<Message>> get_backfill(const BackfillArgs& args) override {
      return Result<std::vector<Message>>::error_with_message("not implemented");
    }
    ~MyDataLoader() override {}
};

AbstractDataLoader* construct_data_loader(const DataLoaderArgs& args) {
  return new MyDataLoader();
}


