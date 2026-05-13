from django.db import models
from django.core.exceptions import ValidationError
from apps.core.models import HotelScopedModel, SoftDeleteModel
import datetime
from django.utils.translation import gettext_lazy as _

class RoomType(HotelScopedModel):
    name_en = models.CharField(max_length=80, verbose_name=_("Name (EN)"), default="")
    name_tr = models.CharField(max_length=80, verbose_name=_("Name (TR)"), default="")
    name_fa = models.CharField(max_length=80, verbose_name=_("Name (FA)"), default="")
    base_rate = models.DecimalField(
        max_digits=10, decimal_places=2, verbose_name=_("Base Rate"), default=0
    )

    class Meta:
        verbose_name = _("Room Type")
        verbose_name_plural = _("Room Types")

    def __str__(self):
        from django.utils.translation import get_language
        lang = get_language()
        if lang == "fa":
            return self.name_fa
        if lang == "tr":
            return self.name_tr
        return self.name_en

    @property
    def name(self):
        return self.__str__()


class RoomStatus(models.TextChoices):
    VACANT = "VACANT", _("Vacant")
    OCCUPIED = "OCCUPIED", _("Occupied")
    DIRTY = "DIRTY", _("Dirty")
    CLEAN = "CLEAN", _("Clean")
    RESERVED = "RESERVED", _("Reserved")
    OUT_OF_ORDER = "OUT_OF_ORDER", _("Out of Order")


class Room(HotelScopedModel, SoftDeleteModel):
    number = models.CharField(max_length=10, verbose_name=_("Room Number"))
    floor = models.IntegerField(default=1, verbose_name=_("Floor"))
    room_type = models.ForeignKey(
        RoomType,
        on_delete=models.PROTECT,
        verbose_name=_("Room Type"),
        related_name="rooms",
    )
    status = models.CharField(
        max_length=20,
        choices=RoomStatus.choices,
        default=RoomStatus.CLEAN,
        verbose_name=_("Status"),
    )
    capacity = models.IntegerField(default=2, verbose_name=_("Capacity"))
    is_clean = models.BooleanField(default=True, verbose_name=_("Is Clean"))
    needs_maintenance = models.BooleanField(default=False, verbose_name=_("Needs Maintenance"))
    needs_inventory = models.BooleanField(default=False, verbose_name=_("Needs Inventory Restock"))
    last_restocked = models.DateTimeField(null=True, blank=True, verbose_name=_("Last Restocked"))
    last_cleaned = models.DateTimeField(
        null=True, blank=True, verbose_name=_("Last Cleaned")
    )

    def clean(self):
        super().clean()
        if (
            not self.pk and Room.objects.filter(hotel_id=self.hotel_id).count() >= 500
        ):  # Increased limit
            raise ValidationError(_("Limit reached."))

    def save(self, *args, **kwargs):
        from django.utils import timezone
        if self.is_clean:
            now = timezone.now()
            should_update = not self.last_cleaned
            if not should_update:
                try:
                    should_update = self.last_cleaned < now - timezone.timedelta(hours=1)
                except TypeError:
                    should_update = True
            if should_update:
                self.last_cleaned = now
        self.clean()
        super().save(*args, **kwargs)

    class Meta:
        unique_together = (("hotel_id", "number"),)
        verbose_name = _("Room")
        verbose_name_plural = _("Rooms")


class Table(HotelScopedModel):
    number = models.CharField(max_length=10, verbose_name=_("Table Number"))
    capacity = models.IntegerField(
        choices=[(2, _("2 Persons")), (4, _("4 Persons")), (6, _("6 Persons"))],
        default=2,
        verbose_name=_("Capacity"),
    )
    status = models.CharField(
        max_length=20,
        choices=[("vacant", _("Vacant")), ("occupied", _("Occupied")), ("reserved", _("Reserved"))],
        default="vacant",
        verbose_name=_("Status"),
    )

    class Meta:
        verbose_name = _("Table")
        verbose_name_plural = _("Tables")
