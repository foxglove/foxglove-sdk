import time

from foxglove import Context
from foxglove.channel import Channel
from foxglove.channels import SceneUpdateChannel
import foxglove.schemas

ctx1 = Context()
ctx2 = Context()

mcap1 = ctx1.open_mcap("file1.mcap")
mcap2 = ctx2.open_mcap("file2.mcap")

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
