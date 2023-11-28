# PJRT C API changelog

## 0.40 (Nov 27, 2023)
* Added PJRT_Executable_GetCompiledMemoryStats.

## 0.39 (Nov 16, 2023)
* Add non_donatable_input_indices and num_non_donatable_input_indices to
PJRT_ExecuteOptions.

## 0.38 (Oct 30, 2023)
* Use `enum` to define STRUCT_SIZE constants in a header file.

## 0.37 (Oct 27, 2023)
* Added const to a bunch of lists and value types.

## 0.36 (Oct 24, 2023)
* Added PJRT_Client_TopologyDescription

## 0.35 (Oct 20, 2023)
* Added PJRT_Executable_Fingerprint method
* Deprecated PJRT_LoadedExecutable_Fingerprint

## 0.34 (Oct 9, 2023)
* Added PJRT_Structure_Type::PJRT_Structure_Type_Profiler.

## 0.33 (Oct 3, 2023)
* Added PJRT_Client_CreateViewOfDeviceBuffer.

## 0.32 (Sep 26, 2023)
* Added PJRT_Buffer_CopyToMemory.

## 0.31 (Sep 22, 2023)
* Added PJRT_Structure_Base.
* Added PJRT_Structure_Type.
* Renamed PJRT_Api.priv to PJRT_Api.extension_start.

## 0.30 (Sep 14, 2023)
* Added PJRT_NamedValue_Type::PJRT_NamedValue_kBool.

## 0.29 (Sep 6, 2023)
* Added PJRT_Executable_OutputElementTypes.
* Added PJRT_Executable_OutputDimensions.
