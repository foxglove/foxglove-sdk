To run the example:

```
export CC=clang
export CXX=clang++
sudo apt-get install protobuf-compiler
mkdir build
cd build
cmake ..
make
./custom_protobuf
```

And then open example.mcap in the Foxglove app
