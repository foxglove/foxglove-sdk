#include "data_loader.hpp"

using namespace foxglove_data_loader;

struct exports_foxglove_loader_loader_message_iterator_t {
    AbstractMessageIterator *message_iterator;
};

struct exports_foxglove_loader_loader_data_loader_t {
    AbstractDataLoader *data_loader;
};

#include "host.h"

void console_log(const char *msg)
{
  host_string_t h_str;
  host_string_dup(&h_str, msg);
  foxglove_loader_console_log(&h_str);
}

Reader Reader::open(const char* path) {
    host_string_t host_path;
    host_string_dup(&host_path, path);
    auto reader = foxglove_loader_reader_open(&host_path);
    return Reader(reader.__handle);
}

uint64_t Reader::size() {
    foxglove_loader_reader_borrow_reader_t reader;
    reader.__handle = this->handle;
    return foxglove_loader_reader_method_reader_size(reader);
}

uint64_t Reader::position() {
    foxglove_loader_reader_borrow_reader_t reader;
    reader.__handle = this->handle;
    return foxglove_loader_reader_method_reader_position(reader);
}

uint64_t Reader::seek(uint64_t pos) {
    foxglove_loader_reader_borrow_reader_t reader;
    reader.__handle = this->handle;
    return foxglove_loader_reader_method_reader_seek(reader, pos);
}

uint64_t Reader::read(uint8_t* into, size_t len) {
    foxglove_loader_reader_borrow_reader_t reader;
    reader.__handle = this->handle;
    host_list_u8_t target;
    target.len = len;
    target.ptr = into;
    return foxglove_loader_reader_method_reader_read(reader, &target);
}

extern void exports_foxglove_loader_loader_message_iterator_destructor(
    exports_foxglove_loader_loader_message_iterator_t *rep)
{
  delete rep->message_iterator;
}


extern void exports_foxglove_loader_loader_data_loader_destructor(
    exports_foxglove_loader_loader_data_loader_t *rep)
{
  delete rep->data_loader;
}

extern bool exports_foxglove_loader_loader_method_message_iterator_next(
    exports_foxglove_loader_loader_borrow_message_iterator_t self,
    exports_foxglove_loader_loader_result_message_error_t *ret)
{
  AbstractMessageIterator *iter = self->message_iterator;
  std::optional<Result<Message>> optional_result = iter->next();
  if (!optional_result.has_value())
  {
    return false;
  }
  Result<Message> result = optional_result.value();
  if (result.value.has_value()) {
    ret->is_err = false;
    Message msg = result.value.value();
    ret->val.ok.channel_id = msg.channel_id;
    ret->val.ok.log_time = msg.log_time;
    ret->val.ok.publish_time = msg.publish_time;
    ret->val.ok.data.len = msg.len;
    ret->val.ok.data.ptr = (uint8_t*) calloc(msg.len, sizeof(uint8_t));
    memcpy(ret->val.ok.data.ptr, msg.ptr, msg.len);
  } else {
    ret->is_err = true;
    host_string_dup(&ret->val.err, result.error.c_str());
  }
  return true;
}

extern exports_foxglove_loader_loader_own_data_loader_t exports_foxglove_loader_loader_constructor_data_loader(
    exports_foxglove_loader_loader_data_loader_args_t *args)
{
  DataLoaderArgs data_loader_args;
  for (size_t i = 0; i < args->paths.len; i++) {
    host_string_t* path = &args->paths.ptr[i];
    data_loader_args.paths.push_back(std::string((char*) path->ptr, path->len));
  }

  return exports_foxglove_loader_loader_data_loader_new(
      new exports_foxglove_loader_loader_data_loader_t{
          .data_loader = construct_data_loader(data_loader_args),
      });
}


extern bool exports_foxglove_loader_loader_method_data_loader_initialize(
    exports_foxglove_loader_loader_borrow_data_loader_t self,
    exports_foxglove_loader_loader_initialization_t *ret,
    exports_foxglove_loader_loader_error_t *err)
{
  Result<Initialization> init_result = self->data_loader->initialize();
  if (!init_result.value.has_value()) {
    host_string_dup(err, init_result.error.c_str());
    return false;
  }
  Initialization init = init_result.value.value();

  ret->channels.len = init.channels.size();
  ret->channels.ptr = (exports_foxglove_loader_loader_channel_t *)malloc(
      sizeof(exports_foxglove_loader_loader_channel_t) * init.channels.size());
  {
    size_t i = 0;
    for (Channel ch : init.channels)
    {
      exports_foxglove_loader_loader_channel_t *h_ch = ret->channels.ptr +
                                                       sizeof(exports_foxglove_loader_loader_channel_t) * (i++);
      h_ch->id = ch.id;
      if (ch.schema_id.has_value())
      {
        h_ch->schema_id.is_some = true;
        h_ch->schema_id.val = ch.schema_id.value();
      }
      else
      {
        h_ch->schema_id.is_some = false;
      }
      host_string_dup(&(h_ch->topic_name), ch.topic_name.c_str());
      host_string_dup(&(h_ch->message_encoding), ch.message_encoding.c_str());
      if (ch.message_count.has_value())
      {
        h_ch->message_count.is_some = true;
        h_ch->message_count.val = ch.message_count.value();
      }
      else
      {
        h_ch->message_count.is_some = false;
      }
    }
  }

  {
    ret->schemas.len = init.schemas.size();
    ret->schemas.ptr = (exports_foxglove_loader_loader_schema_t *)calloc(
        init.schemas.size(),
        sizeof(exports_foxglove_loader_loader_schema_t)
    );
    size_t i = 0;
    for (Schema schema : init.schemas)
    {
      exports_foxglove_loader_loader_schema_t *h_schema = ret->schemas.ptr +
                                                          sizeof(exports_foxglove_loader_loader_schema_t) * (i++);
      h_schema->id = schema.id;
      host_string_dup(&(h_schema->name), schema.name.c_str());
      host_string_dup(&(h_schema->encoding), schema.encoding.c_str());
      h_schema->data.len = schema.data_len;
      h_schema->data.ptr = (uint8_t *) calloc(schema.data_len, sizeof(uint8_t));
      memcpy(h_schema->data.ptr, schema.data, schema.data_len);
    }
  }

  ret->time_range.start_time = init.time_range.start_time;
  ret->time_range.end_time = init.time_range.end_time;

  {
    ret->problems.len = init.problems.size();
    ret->problems.ptr = (exports_foxglove_loader_loader_problem_t *)malloc(sizeof(exports_foxglove_loader_loader_problem_t *) * init.problems.size());
    size_t i = 0;
    for (std::string problem : init.problems)
    {
      exports_foxglove_loader_loader_problem_t *hs_problem = ret->problems.ptr + sizeof(exports_foxglove_loader_loader_problem_t *) * (i++);
      host_string_dup(&hs_problem->message, problem.c_str());
      hs_problem->severity = EXPORTS_FOXGLOVE_LOADER_LOADER_SEVERITY_ERROR;
      hs_problem->tip.is_some = false;
    }
  }
  return true;
}

extern bool exports_foxglove_loader_loader_method_data_loader_create_iterator(
    exports_foxglove_loader_loader_borrow_data_loader_t self,
    exports_foxglove_loader_loader_message_iterator_args_t *args,
    exports_foxglove_loader_loader_own_message_iterator_t *ret,
    exports_foxglove_loader_loader_error_t *err)
{
  MessageIteratorArgs iter_args;
  if (args->start_time.is_some)
  {
    iter_args.start_time.emplace(args->start_time.val);
  }
  if (args->end_time.is_some)
  {
    iter_args.end_time.emplace(args->end_time.val);
  }
  for (size_t i = 0; i < args->channels.len; i++)
  {
    ChannelId *ch_id = &args->channels.ptr[i];
    iter_args.channel_ids.push_back(*ch_id);
  }
  Result<AbstractMessageIterator*> iter_result = self->data_loader->create_iterator(iter_args);
  if (iter_result.value.has_value()) {
    AbstractMessageIterator* iter = iter_result.value.value();
    ret->__handle = (int32_t) new exports_foxglove_loader_loader_message_iterator_t{
        .message_iterator = iter,
    };
    return true;
  } else {
    host_string_dup(err, iter_result.error.c_str());
    return false;
  }
}

extern bool exports_foxglove_loader_loader_method_data_loader_get_backfill(
    exports_foxglove_loader_loader_borrow_data_loader_t self,
    exports_foxglove_loader_loader_backfill_args_t *args,
    exports_foxglove_loader_loader_list_message_t *ret,
    exports_foxglove_loader_loader_error_t *err)
{
  BackfillArgs backfill_args;
  for (size_t i = 0; i < args->channels.len; i++) {
    backfill_args.channel_ids.push_back(args->channels.ptr[i]);
  }
  backfill_args.time = args->time;
  Result<std::vector<Message>> backfill_result = self->data_loader->get_backfill(backfill_args);
  if (backfill_result.ok()) {
    auto& messages = backfill_result.get();
    size_t len = messages.size();
    ret->ptr = (exports_foxglove_loader_loader_message_t*) calloc(len, sizeof(exports_foxglove_loader_loader_message_t));
    ret->len = len;
    for (size_t i = 0; i < len; i++) {
        exports_foxglove_loader_loader_message_t* ret_message = &ret->ptr[i];
        Message& message = messages[i];
        ret_message->channel_id = message.channel_id;
        ret_message->log_time = message.log_time;
        ret_message->publish_time = message.publish_time;
        ret_message->data.ptr = (uint8_t*) calloc(message.len, sizeof(uint8_t));
        memcpy(ret_message->data.ptr, message.ptr, message.len);
        ret_message->data.len = message.len;
    }
    return true;
  } else {
    host_string_dup(err, backfill_result.error.c_str());
    return false;
  }
}
