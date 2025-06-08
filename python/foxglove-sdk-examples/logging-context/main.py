import time

import foxglove.schemas
from foxglove import Channel, Context, open_mcap
from foxglove.channels import SceneUpdateChannel

ctx1 = Context()
ctx2 = Context()

mcap1 = open_mcap("file1.mcap", context=ctx1)
mcap2 = open_mcap("file2.mcap", context=ctx2)

foo = SceneUpdateChannel("/foo", context=ctx1)
bar = Channel("/bar", context=ctx1)
baz = Channel("/baz", context=ctx2)

for _ in range(10):
    # Log /foo and /bar to mcap1, and /baz to mcap2
    foo.log(foxglove.schemas.SceneUpdate())
    bar.log({"hello": "world"})
    baz.log({"hello": "world"})
    time.sleep(0.1)

mcap1.close()
mcap2.close()
