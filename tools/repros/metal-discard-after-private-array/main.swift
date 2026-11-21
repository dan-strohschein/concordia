import Metal
let device = MTLCreateSystemDefaultDevice()!
let source = try! String(contentsOfFile: CommandLine.arguments[1], encoding: .utf8)
let library = try! device.makeLibrary(source: source, options: nil)
let desc = MTLRenderPipelineDescriptor()
desc.vertexFunction = library.makeFunction(name: "vs")
desc.fragmentFunction = library.makeFunction(name: "fs")
desc.colorAttachments[0].pixelFormat = .rgba8Unorm
let pipeline = try! device.makeRenderPipelineState(descriptor: desc)
let data: [Float] = [1, 0, 0, 0.78]
let buffer = device.makeBuffer(bytes: data, length: 16)!
let queue = device.makeCommandQueue()!
for mode: UInt32 in [0, 1] {
    let td = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .rgba8Unorm, width: 4, height: 4, mipmapped: false)
    td.usage = [.renderTarget]; td.storageMode = .shared
    let texture = device.makeTexture(descriptor: td)!
    let pass = MTLRenderPassDescriptor()
    pass.colorAttachments[0].texture = texture
    pass.colorAttachments[0].loadAction = .clear
    pass.colorAttachments[0].clearColor = MTLClearColor(red: 0, green: 0, blue: 1, alpha: 1)
    pass.colorAttachments[0].storeAction = .store
    let commands = queue.makeCommandBuffer()!
    let encoder = commands.makeRenderCommandEncoder(descriptor: pass)!
    encoder.setRenderPipelineState(pipeline)
    encoder.setFragmentBuffer(buffer, offset: 0, index: 0)
    var params: [UInt32] = [1, mode, 0, 0]
    encoder.setFragmentBytes(&params, length: 16, index: 1)
    encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
    encoder.endEncoding()
    commands.commit(); commands.waitUntilCompleted()
    var pixel = [UInt8](repeating: 0, count: 4)
    texture.getBytes(&pixel, bytesPerRow: 16, from: MTLRegionMake2D(1, 1, 1, 1), mipmapLevel: 0)
    print("mode \(mode) (\(mode == 0 ? "alpha from the array" : "alpha from the buffer")): \(pixel) \(pixel == [255, 0, 0, 255] ? "drawn (expected)" : "DISCARDED (wrong)")")
}
print(device.name)
