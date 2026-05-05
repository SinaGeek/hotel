from django.db import models
from apps.core.models import HotelScopedModel
class Stay(HotelScopedModel):
    reservation=models.OneToOneField('reservations.Reservation',on_delete=models.PROTECT)
    room=models.ForeignKey('rooms.Room',on_delete=models.PROTECT)
    actual_check_in=models.DateTimeField(null=True, blank=True); actual_check_out=models.DateTimeField(null=True, blank=True)
