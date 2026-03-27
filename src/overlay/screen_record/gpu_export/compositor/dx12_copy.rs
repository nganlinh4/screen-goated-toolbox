use std::mem::ManuallyDrop;
use windows::Win32::Graphics::Direct3D12 as d3d12;
use windows::core::Interface;

pub(super) const READBACK_RING_SIZE: usize = 5;

pub(in crate::overlay::screen_record::gpu_export) struct Dx12SharedCopyContext {
    device: d3d12::ID3D12Device,
    queue: d3d12::ID3D12CommandQueue,
}

impl Dx12SharedCopyContext {
    pub(super) unsafe fn new(
        device: &d3d12::ID3D12Device,
        queue: &d3d12::ID3D12CommandQueue,
    ) -> Result<Self, String> {
        Ok(Self {
            device: device.clone(),
            queue: queue.clone(),
        })
    }

    pub(super) unsafe fn texture_raw_resource(
        texture: &wgpu::Texture,
    ) -> Option<d3d12::ID3D12Resource> {
        unsafe {
            let hal_texture = texture.as_hal::<wgpu::hal::api::Dx12>()?;
            let resource_058 = hal_texture.raw_resource();
            let resource_062: &d3d12::ID3D12Resource = &*(resource_058 as *const _);
            Some(resource_062.clone())
        }
    }

    fn transition_barrier(
        resource: Option<d3d12::ID3D12Resource>,
        before: d3d12::D3D12_RESOURCE_STATES,
        after: d3d12::D3D12_RESOURCE_STATES,
    ) -> d3d12::D3D12_RESOURCE_BARRIER {
        d3d12::D3D12_RESOURCE_BARRIER {
            Type: d3d12::D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
            Flags: d3d12::D3D12_RESOURCE_BARRIER_FLAG_NONE,
            Anonymous: d3d12::D3D12_RESOURCE_BARRIER_0 {
                Transition: ManuallyDrop::new(d3d12::D3D12_RESOURCE_TRANSITION_BARRIER {
                    pResource: ManuallyDrop::new(resource),
                    Subresource: d3d12::D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                    StateBefore: before,
                    StateAfter: after,
                }),
            },
        }
    }

    fn global_uav_barrier() -> d3d12::D3D12_RESOURCE_BARRIER {
        d3d12::D3D12_RESOURCE_BARRIER {
            Type: d3d12::D3D12_RESOURCE_BARRIER_TYPE_UAV,
            Flags: d3d12::D3D12_RESOURCE_BARRIER_FLAG_NONE,
            Anonymous: d3d12::D3D12_RESOURCE_BARRIER_0 {
                UAV: ManuallyDrop::new(d3d12::D3D12_RESOURCE_UAV_BARRIER {
                    pResource: ManuallyDrop::new(None),
                }),
            },
        }
    }

    pub(super) unsafe fn copy_shared_to_video(
        &self,
        source: &wgpu::Texture,
        video_texture: &wgpu::Texture,
    ) -> Result<(), String> {
        unsafe {
            let source_resource =
                Self::texture_raw_resource(source).ok_or("Source texture has no DX12 resource")?;
            let video_resource = Self::texture_raw_resource(video_texture)
                .ok_or("Video texture has no DX12 resource")?;

            let shader_read = d3d12::D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE
                | d3d12::D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE;

            let allocator = self
                .device
                .CreateCommandAllocator::<d3d12::ID3D12CommandAllocator>(
                    d3d12::D3D12_COMMAND_LIST_TYPE_DIRECT,
                )
                .map_err(|e| format!("CreateCommandAllocator: {e}"))?;
            let command_list = self
                .device
                .CreateCommandList::<_, _, d3d12::ID3D12GraphicsCommandList>(
                    0,
                    d3d12::D3D12_COMMAND_LIST_TYPE_DIRECT,
                    &allocator,
                    None,
                )
                .map_err(|e| format!("CreateCommandList: {e}"))?;

            let pre_barriers = [
                Self::transition_barrier(
                    Some(source_resource.clone()),
                    d3d12::D3D12_RESOURCE_STATE_COMMON,
                    d3d12::D3D12_RESOURCE_STATE_COPY_SOURCE,
                ),
                Self::transition_barrier(
                    Some(video_resource.clone()),
                    shader_read,
                    d3d12::D3D12_RESOURCE_STATE_COPY_DEST,
                ),
                Self::global_uav_barrier(),
            ];
            command_list.ResourceBarrier(&pre_barriers);
            command_list.CopyResource(&video_resource, &source_resource);

            let post_barriers = [
                Self::transition_barrier(
                    Some(video_resource),
                    d3d12::D3D12_RESOURCE_STATE_COPY_DEST,
                    shader_read,
                ),
                Self::transition_barrier(
                    Some(source_resource),
                    d3d12::D3D12_RESOURCE_STATE_COPY_SOURCE,
                    d3d12::D3D12_RESOURCE_STATE_COMMON,
                ),
            ];
            command_list.ResourceBarrier(&post_barriers);

            command_list
                .Close()
                .map_err(|e| format!("CommandList::Close: {e}"))?;

            let command_list_base: d3d12::ID3D12CommandList = command_list
                .cast()
                .map_err(|e| format!("CommandList cast: {e}"))?;
            self.queue.ExecuteCommandLists(&[Some(command_list_base)]);
            Ok(())
        }
    }
}
