package xyz.xenondevs.tessera.capture.dump.sink

import com.google.gson.GsonBuilder
import com.google.gson.JsonArray
import com.google.gson.JsonNull
import com.google.gson.JsonObject
import com.google.gson.JsonPrimitive
import it.unimi.dsi.fastutil.objects.Object2IntLinkedOpenHashMap
import net.minecraft.resources.Identifier
import org.joml.Matrix4fc
import org.joml.Vector2fc
import org.joml.Vector3fc
import xyz.xenondevs.tessera.capture.dump.CaptureResult
import xyz.xenondevs.tessera.capture.dump.CapturedCube
import xyz.xenondevs.tessera.capture.dump.CapturedMaterial
import xyz.xenondevs.tessera.capture.dump.CapturedPrimitive
import xyz.xenondevs.tessera.capture.dump.CapturedSubmission
import xyz.xenondevs.tessera.capture.dump.CapturedTexture
import java.util.*

object RawRecordWriter {
    const val FORMAT = 1
    
    private val GSON = GsonBuilder()
        .disableHtmlEscaping()
        .setPrettyPrinting()
        .create()
    
    private val HEX = HexFormat.of().withUpperCase()
    
    // nicer formatting for number json arrays
    private val NUMBER_ARRAY = Regex("""\[\s+(-?\d[\d.E-]*(?:,\s+-?\d[\d.E-]*)*)\s+]""")
    private val SEPARATOR = Regex(""",\s+""")
    
    // -0.0 -> 0.0
    private val NEGATIVE_ZERO = Regex("""(?<=[\[\s])-0\.0(?![\d.E])""")
    
    fun write(record: BlockRecord, minecraftVersion: String): String {
        val materials = Object2IntLinkedOpenHashMap<CapturedMaterial>()
        @Suppress("DestructuringDeclaration")
        for (state in record.states) {
            val result = state.result as? CaptureResult.Captured ?: continue
            for (submission in result.submissions)
                when (submission) {
                    is CapturedSubmission.Model -> materials.putIfAbsent(submission.material, materials.size)
                    is CapturedSubmission.BlockModel -> submission.quads.forEach { materials.putIfAbsent(it.material, materials.size) }
                }
        }
        
        fun Identifier.asJson() = JsonPrimitive(toString())
        
        fun Vector3fc.asJson() = JsonArray(3).apply {
            add(x())
            add(y())
            add(z())
        }
        
        fun Vector2fc.asJson() = JsonArray(2).apply {
            add(x())
            add(y())
        }
        
        fun Matrix4fc.asJson() = JsonArray(12).apply {
            floatArrayOf(m00(), m10(), m20(), m30(), m01(), m11(), m21(), m31(), m02(), m12(), m22(), m32()).forEach { add(it) }
        }
        
        fun CapturedCube.asJson() = JsonObject().apply {
            add("from", from.asJson())
            add("to", to.asJson())
            add("faces", JsonObject().apply {
                for ((direction, uvs) in faces)
                    add(direction.serializedName, JsonArray(4).apply { uvs.forEach { add(it.asJson()) } })
            })
        }
        
        fun CapturedMaterial.asJson() = JsonObject().apply {
            val textureJson = when (val source = texture) {
                is CapturedTexture.Sprite -> JsonObject().apply {
                    add("sprite", source.id.asJson())
                }
                
                is CapturedTexture.Standalone -> JsonObject().apply {
                    add("path", source.path.asJson())
                }
            }
            val depthBiasJson = depthBias
                ?.let {
                    JsonArray(2).apply {
                        add(it.scale)
                        add(it.constant)
                    }
                } ?: JsonNull.INSTANCE
            
            add("texture", textureJson)
            add("pipeline", pipeline.asJson())
            addProperty("cull", cull)
            addProperty("blend", blend)
            add("alpha_cutout", alphaCutout?.let(::JsonPrimitive) ?: JsonNull.INSTANCE)
            addProperty("depth_write", depthWrite)
            add("depth_bias", depthBiasJson)
            addProperty("layering", layering.name.lowercase())
            add("tint_index", tintIndex?.let(::JsonPrimitive) ?: JsonNull.INSTANCE)
            add("tint_color", tintColor?.let { JsonPrimitive("#" + HEX.toHexDigits(it)) } ?: JsonNull.INSTANCE)
            add("shade", shade?.let { JsonPrimitive(it.serializedName) } ?: JsonNull.INSTANCE)
            addProperty("light_emission", lightEmission)
        }
        
        fun CapturedPrimitive.asJson() = JsonObject().apply {
            val kind = when (this@asJson) {
                is CapturedPrimitive.Parallelogram -> "parallelogram"
                is CapturedPrimitive.Triangle -> "triangle"
            }
            addProperty("kind", kind)
            addProperty("material", materials.getInt(material))
            add("positions", JsonArray(positions.size).apply { positions.forEach { add(it.asJson()) } })
            add("uvs", JsonArray(uvs.size).apply { uvs.forEach { add(it.asJson()) } })
            add("normal", normal.asJson())
        }
        
        fun CapturedSubmission.asJson() = JsonObject().apply {
            when (val submission = this@asJson) {
                is CapturedSubmission.Model -> {
                    addProperty("kind", "model")
                    addProperty("order", submission.order)
                    addProperty("material", materials.getInt(submission.material))
                    add("pose", submission.pose.asJson())
                    add("parts", JsonArray(submission.parts.size).apply {
                        for ((path, innerPose, cubes) in submission.parts) {
                            add(JsonObject().apply {
                                addProperty("path", path)
                                add("pose", innerPose.asJson())
                                add("cubes", JsonArray(cubes.size).apply { cubes.forEach { add(it.asJson()) } })
                            })
                        }
                    })
                }
                
                is CapturedSubmission.BlockModel -> {
                    addProperty("kind", "block_model")
                    addProperty("order", submission.order)
                    add("pose", submission.pose.asJson())
                    add("quads", JsonArray(submission.quads.size).apply { submission.quads.forEach { add(it.asJson()) } })
                }
            }
        }
        
        val states = JsonObject()
        for ((key, renderShape, result) in record.states) {
            val stateObject = JsonObject().apply {
                addProperty("render_shape", renderShape.name.lowercase())
                when (result) {
                    CaptureResult.NoBlockEntityRenderer -> Unit
                    is CaptureResult.Captured -> {
                        add("capture", JsonArray(result.submissions.size).apply { result.submissions.forEach { add(it.asJson()) } })
                    }
                    
                    is CaptureResult.Discarded -> {
                        add("discarded", JsonArray(result.reasons.size).apply { result.reasons.forEach { add(JsonPrimitive(it.name.lowercase())) } })
                    }
                }
            }
            states.add(key, stateObject)
        }
        
        val out = JsonObject().apply {
            addProperty("format", FORMAT)
            addProperty("minecraft", minecraftVersion)
            add("block", record.block.asJson())
            add("materials", JsonArray(materials.size).apply { materials.keys.forEach { add(it.asJson()) } })
            add("states", states)
        }
        
        val json = NUMBER_ARRAY.replace(GSON.toJson(out)) { "[" + it.groupValues[1].replace(SEPARATOR, ", ") + "]" }
        return NEGATIVE_ZERO.replace(json, "0.0")
    }
    
}
