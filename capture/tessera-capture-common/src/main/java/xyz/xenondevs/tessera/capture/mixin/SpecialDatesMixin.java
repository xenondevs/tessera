package xyz.xenondevs.tessera.capture.mixin;

import net.minecraft.util.SpecialDates;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

import java.time.Month;
import java.time.MonthDay;

@Mixin(SpecialDates.class)
public class SpecialDatesMixin {
    
    @Inject(method = "dayNow", at = @At("HEAD"), cancellable = true)
    private static void pinDate(CallbackInfoReturnable<MonthDay> cir) {
        cir.setReturnValue(MonthDay.of(Month.JUNE, 15));
    }
    
}
