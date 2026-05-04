from django.db import models
from apps.core.models import HotelScopedModel, SoftDeleteModel
class RoomType(HotelScopedModel):
    name=models.CharField(max_length=80); base_rate=models.DecimalField(max_digits=10, decimal_places=2)
class RoomStatus(models.TextChoices):
    VACANT='VACANT'; OCCUPIED='OCCUPIED'; DIRTY='DIRTY'; CLEAN='CLEAN'; RESERVED='RESERVED'; OUT_OF_ORDER='OUT_OF_ORDER'
class Room(HotelScopedModel, SoftDeleteModel):
    number=models.CharField(max_length=10); room_type=models.ForeignKey(RoomType,on_delete=models.PROTECT)
    status=models.CharField(max_length=20, choices=RoomStatus.choices, default=RoomStatus.CLEAN)
    class Meta: unique_together=(('hotel_id','number'),)
