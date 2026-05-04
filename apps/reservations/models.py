from django.db import models
from apps.core.models import HotelScopedModel
from apps.rooms.models import Room
from apps.guests.models import Guest
class Source(models.TextChoices): FRONT_DESK='FRONT_DESK'; BOOKING='BOOKING'; EXPEDIA='EXPEDIA'; WALK_IN='WALK_IN'
class Reservation(HotelScopedModel):
    guest=models.ForeignKey(Guest,on_delete=models.PROTECT)
    room=models.ForeignKey(Room,on_delete=models.PROTECT)
    check_in_date=models.DateField(); check_out_date=models.DateField()
    source=models.CharField(max_length=20, choices=Source.choices); external_ref=models.CharField(max_length=120, blank=True)
    status=models.CharField(max_length=20, default='CONFIRMED')
