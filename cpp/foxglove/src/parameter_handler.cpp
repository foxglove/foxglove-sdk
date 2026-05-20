#include <foxglove-c/foxglove-c.h>
#include <foxglove/parameter_handler.hpp>

namespace foxglove {

/**
 * GetParametersResponder implementation.
 */
void GetParametersResponder::Deleter::operator()(foxglove_get_parameters_responder* ptr
) const noexcept {
  foxglove_get_parameters_responder_drop(ptr);
}

void GetParametersResponder::respond(std::vector<Parameter>&& params) && noexcept {
  ParameterArray array(std::move(params));
  foxglove_get_parameters_responder_respond(impl_.release(), array.release());
}

/**
 * SetParametersResponder implementation.
 */
void SetParametersResponder::Deleter::operator()(foxglove_set_parameters_responder* ptr
) const noexcept {
  foxglove_set_parameters_responder_drop(ptr);
}

void SetParametersResponder::respond(std::vector<Parameter>&& params) && noexcept {
  ParameterArray array(std::move(params));
  foxglove_set_parameters_responder_respond(impl_.release(), array.release());
}

}  // namespace foxglove
