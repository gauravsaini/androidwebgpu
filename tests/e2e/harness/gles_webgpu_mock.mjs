/**
 * WebGPU Canvas & Swapchain Environment Mock for Headless E2E Execution
 * Strict compliance with F-GPU-03 (strictly no putImageData) and F-GPU-06 (device loss recovery).
 */

export class MockGPUDevice {
  constructor() {
    this._lostDeferred = {};
    this.lost = new Promise((resolve, reject) => {
      this._lostDeferred.resolve = resolve;
      this._lostDeferred.reject = reject;
    });
    this.queue = {
      submit: (commandBuffers) => {
        this.submittedBatches = (this.submittedBatches || 0) + commandBuffers.length;
      },
      writeTexture: (destination, data, dataLayout, size) => {
        this.lastWrittenTexture = { destination, size, byteLength: data.byteLength };
      }
    };
    this.textures = [];
    this.destroyed = false;
  }

  createTexture(descriptor) {
    if (this.destroyed) throw new Error('DEVICE_DESTROYED');
    const texture = {
      descriptor,
      destroyed: false,
      destroy: () => { texture.destroyed = true; },
      createView: () => ({ texture, format: descriptor.format })
    };
    this.textures.push(texture);
    return texture;
  }

  createCommandEncoder() {
    return {
      beginRenderPass: () => ({
        setPipeline: () => {},
        setBindGroup: () => {},
        draw: () => {},
        drawIndexed: () => {},
        end: () => {}
      }),
      finish: () => ({ type: 'command_buffer' })
    };
  }

  destroy() {
    this.destroyed = true;
    this._lostDeferred.resolve({ reason: 'destroyed', message: 'Device was destroyed explicitly' });
  }

  simulateDeviceLoss(reason = 'unknown', message = 'GPU connection lost') {
    this.destroyed = true;
    this._lostDeferred.resolve({ reason, message });
  }
}

export class MockGPUCanvasContext {
  constructor(canvas) {
    this.canvas = canvas;
    this.configured = false;
    this.configuration = null;
    this.currentTexture = null;
    this.presentCount = 0;
  }

  configure(config) {
    if (!config.device) throw new Error('CONFIG_MISSING_DEVICE');
    if (!['bgra8unorm', 'rgba8unorm'].includes(config.format)) {
      throw new Error(`UNSUPPORTED_SURFACE_FORMAT:${config.format}`);
    }
    this.configuration = config;
    this.configured = true;
  }

  getCurrentTexture() {
    if (!this.configured) throw new Error('CONTEXT_NOT_CONFIGURED');
    if (this.configuration.device.destroyed) throw new Error('DEVICE_LOST');
    this.currentTexture = this.configuration.device.createTexture({
      size: [this.canvas.width, this.canvas.height, 1],
      format: this.configuration.format,
      usage: 0x10 // RENDER_ATTACHMENT
    });
    this.presentCount += 1;
    return this.currentTexture;
  }

  unconfigure() {
    this.configured = false;
    this.configuration = null;
  }
}

export class MockCanvasElement {
  constructor(width = 800, height = 600) {
    this.width = width;
    this.height = height;
    this.style = {};
    this.putImageDataCalls = 0;
    this._gpuContext = new MockGPUCanvasContext(this);
    this._2dContext = {
      putImageData: () => {
        this.putImageDataCalls += 1;
        throw new Error('FORBIDDEN_CALL:putImageData is strictly prohibited in WebGPU pipeline');
      }
    };
  }

  getContext(type) {
    if (type === 'webgpu') return this._gpuContext;
    if (type === '2d') return this._2dContext;
    return null;
  }
}
