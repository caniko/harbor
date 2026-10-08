# mkGpuRenderPin

`mkGpuRenderPin` returns a devShell-ready GPU render profile for projects whose
visual snapshots depend on a stable renderer and driver. It is intended for test
harnesses that compare pixels or structural image similarity and need to fail
fast when a developer is using a different GPU stack than the baseline author.

## Usage

```nix
visualTestGpuPin = rs-harbor.lib.mkGpuRenderPin {
  inherit pkgs;
  profile = "mesa-radv";
};

devShells = rs-harbor.lib.mkDevShells {
  inherit pkgs cross;
  inherit (toolchain) craneLib;

  packages = with pkgs; [ just ] ++ visualTestGpuPin.packages;
  extraEnv = visualTestGpuPin.env // {
    LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
      pkgs.vulkan-loader
      pkgs.libxkbcommon
    ];
  };
};
```

The consuming test framework should read the exported `RS_HARBOR_GPU_*`
variables and compare them with the renderer identity reported by the graphics
API it actually initialized.

## Parameters

| Param      | Default       | Description                                                                             |
| ---------- | ------------- | --------------------------------------------------------------------------------------- |
| `pkgs`     | required      | nixpkgs package set used to provide the graphics stack                                  |
| `profile`  | `"mesa-radv"` | Named render profile                                                                    |
| `expected` | `{}`          | Extra or overriding expected identity fields. Attributes with `null` values are ignored |

Returns: `{ env, packages }`

## Profiles

### `mesa-radv`

Pins visual rendering to Vulkan on Mesa RADV:

- `RS_HARBOR_GPU_PIN_PROFILE=mesa-radv`
- `RS_HARBOR_GPU_EXPECTED_BACKEND=Vulkan`
- `RS_HARBOR_GPU_EXPECTED_VENDOR=0x1002`
- `RS_HARBOR_GPU_EXPECTED_DRIVER_CONTAINS=radv`
- `WGPU_BACKEND=vulkan`
- `VK_DRIVER_FILES=<pkgs.mesa>/share/vulkan/icd.d/radeon_icd.<system>.json`

The profile intentionally checks the AMD vendor and RADV driver family without
pinning a specific GPU device ID. That keeps one baseline usable across AMD
machines while still rejecting NVIDIA, Intel, lavapipe, llvmpipe, Metal, DX12,
and GL renderers for that baseline.

Supported RADV ICD names are currently defined for `x86_64-linux`,
`i686-linux`, and `aarch64-linux`.

## Expected Identity Contract

Consumers may use these environment variables:

- `RS_HARBOR_GPU_PIN_PROFILE`
- `RS_HARBOR_GPU_EXPECTED_BACKEND`
- `RS_HARBOR_GPU_EXPECTED_VENDOR`
- `RS_HARBOR_GPU_EXPECTED_DEVICE`
- `RS_HARBOR_GPU_EXPECTED_DEVICE_TYPE`
- `RS_HARBOR_GPU_EXPECTED_NAME_CONTAINS`
- `RS_HARBOR_GPU_EXPECTED_DRIVER_CONTAINS`
- `RS_HARBOR_GPU_EXPECTED_DRIVER_INFO_CONTAINS`

`mkGpuRenderPin` sets only the fields that are meaningful for the selected
profile. Downstream projects can add stricter checks with `expected`, for
example:

```nix
rs-harbor.lib.mkGpuRenderPin {
  inherit pkgs;
  profile = "mesa-radv";
  expected.RS_HARBOR_GPU_EXPECTED_DEVICE_TYPE = "DiscreteGpu";
}
```
