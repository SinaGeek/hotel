from django.db import models
from apps.core.models import HotelScopedModel

class HousekeepingTask(HotelScopedModel):
    room=models.ForeignKey('rooms.Room',on_delete=models.PROTECT)
    status=models.CharField(max_length=20, default='PENDING')
    priority=models.IntegerField(default=3)
    assigned_to=models.ForeignKey('accounts.User',null=True,blank=True,on_delete=models.SET_NULL)

class LostAndFound(HotelScopedModel):
    room=models.ForeignKey('rooms.Room', on_delete=models.SET_NULL, null=True, blank=True)
    item_name=models.CharField(max_length=200)
    description=models.TextField(blank=True)
    photo=models.ImageField(upload_to='housekeeping/lost_found/', null=True, blank=True)
    found_date=models.DateTimeField(auto_now_add=True)
    found_by=models.ForeignKey('accounts.User', on_delete=models.SET_NULL, null=True)
    is_returned=models.BooleanField(default=False)
    return_notes=models.TextField(blank=True)
