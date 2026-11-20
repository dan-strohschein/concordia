# bad-abi-guest

A native module library that reports ABI version 999, so `concordia-script-native` can show it refuses modules built for another ABI before touching their function table. Built by `just native-fixtures`.
