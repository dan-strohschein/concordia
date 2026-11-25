// Concordia's size shim: the C++ compiler's sizes of the structs the Rust bindings mirror.
#include "meshoptimizer.h"

extern "C" size_t cm_struct_size(unsigned int which)
{
	switch (which)
	{
	case 0:
		return sizeof(meshopt_Meshlet);
	case 1:
		return sizeof(meshopt_Bounds);
	default:
		return 0;
	}
}

extern "C" int cm_version(void)
{
	return MESHOPTIMIZER_VERSION;
}
