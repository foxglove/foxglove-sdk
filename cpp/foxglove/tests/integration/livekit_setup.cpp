#include <livekit/livekit.h>

#include <catch2/catch_test_macros.hpp>
#include <catch2/reporters/catch_reporter_event_listener.hpp>
#include <catch2/reporters/catch_reporter_registrars.hpp>

class LiveKitSetup : public Catch::EventListenerBase {
public:
  using Catch::EventListenerBase::EventListenerBase;

  void testRunStarting(Catch::TestRunInfo const&) override {
    livekit::initialize();
  }

  void testRunEnded(Catch::TestRunStats const&) override {
    livekit::shutdown();
  }
};

CATCH_REGISTER_LISTENER(LiveKitSetup)
