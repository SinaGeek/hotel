from django.db import models
from apps.core.models import HotelScopedModel
class HousekeepingTask(HotelScopedModel):
    room=models.ForeignKey('rooms.Room',on_delete=models.PROTECT)
    status=models.CharField(max_length=20, default='PENDING')
    priority=models.IntegerField(default=3)
    assigned_to=models.ForeignKey('accounts.User',null=True,blank=True,on_delete=models.SET_NULL)
