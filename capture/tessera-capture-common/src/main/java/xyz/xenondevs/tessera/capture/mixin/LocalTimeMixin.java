package xyz.xenondevs.tessera.capture.mixin;

import net.minecraft.client.renderer.item.properties.select.LocalTime;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.ModifyArg;

import java.time.LocalDate;
import java.time.Month;
import java.time.ZoneId;
import java.util.Date;

@Mixin(LocalTime.class)
public class LocalTimeMixin {
    
    @Unique
    private static final Date tessera$PINNED = Date.from(LocalDate.of(2000, Month.JUNE, 15).atTime(12, 0).atZone(ZoneId.systemDefault()).toInstant());
    
    @ModifyArg(method = "update", at = @At(value = "INVOKE", target = "Lcom/ibm/icu/text/DateFormat;format(Ljava/util/Date;)Ljava/lang/String;"))
    private Date pinDate(Date now) {
        return tessera$PINNED;
    }
    
    
}
