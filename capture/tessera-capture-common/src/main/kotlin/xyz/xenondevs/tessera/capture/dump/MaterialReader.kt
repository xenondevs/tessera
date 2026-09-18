package xyz.xenondevs.tessera.capture.dump

import com.mojang.renderpearl.api.pipeline.CompareOp
import net.minecraft.client.renderer.rendertype.LayeringTransform
import net.minecraft.client.renderer.rendertype.RenderType
import net.minecraft.client.renderer.rendertype.TextureTransform
import net.minecraft.client.renderer.texture.TextureAtlasSprite
import net.minecraft.client.renderer.texture.UvMapping
import net.minecraft.core.Direction

const val MAIN_SAMPLER = "Sampler0"

internal sealed interface ReadMaterial {
    
    data class Described(val material: CapturedMaterial) : ReadMaterial
    
    data class Undescribable(val reason: DiscardReason) : ReadMaterial
    
}

internal fun RenderType.readMaterial(
    uvMapping: UvMapping?,
    tintIndex: Int?,
    tintColor: Int?,
    shade: Direction?,
    lightEmission: Int
): ReadMaterial {
    val setup = state
    val pipeline = pipeline()
    
    if (setup.textures.keys.any { it != MAIN_SAMPLER } || setup.textureTransform !== TextureTransform.DEFAULT_TEXTURING)
        return ReadMaterial.Undescribable(DiscardReason.UNSUPPORTED_RENDER_SETUP)
    
    val layering = when (setup.layeringTransform) {
        LayeringTransform.NO_LAYERING -> Layering.NONE
        LayeringTransform.VIEW_OFFSET_Z_LAYERING -> Layering.VIEW_OFFSET_Z
        LayeringTransform.VIEW_OFFSET_Z_LAYERING_FORWARD -> Layering.VIEW_OFFSET_Z_FORWARD
        else -> return ReadMaterial.Undescribable(DiscardReason.UNSUPPORTED_RENDER_SETUP)
    }
    
    val depth = pipeline.depthStencilState?.takeIf { it.depthTest == CompareOp.GREATER_THAN_OR_EQUAL }
        ?: return ReadMaterial.Undescribable(DiscardReason.UNSUPPORTED_RENDER_SETUP)
    
    val depthBias = if (depth.depthBiasScaleFactor != 0f || depth.depthBiasConstant != 0f) {
        DepthBias(depth.depthBiasScaleFactor, depth.depthBiasConstant)
    } else {
        null
    }
    
    val texture = when (uvMapping) {
        is TextureAtlasSprite -> CapturedTexture.Sprite(uvMapping.contents().name())
        null -> setup.textures[MAIN_SAMPLER]?.location?.let(CapturedTexture::Standalone)
            ?: return ReadMaterial.Undescribable(DiscardReason.UNMAPPED_MODEL)
        
        else -> return ReadMaterial.Undescribable(DiscardReason.UNMAPPED_MODEL)
    }
    
    return ReadMaterial.Described(
        CapturedMaterial(
            texture,
            pipeline.location,
            pipeline.isCull,
            hasBlending(),
            pipeline.shaderDefines.values["ALPHA_CUTOUT"]?.toFloat(),
            depth.writeDepth,
            depthBias,
            layering,
            tintIndex,
            tintColor,
            shade,
            lightEmission
        )
    )
}
